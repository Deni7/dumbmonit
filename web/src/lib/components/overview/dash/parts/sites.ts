/**
 * Pure helpers of the site views (honeycomb, sunburst, matrix, site cards,
 * top consumers, backups, services): counting, grouping, ordering, and the
 * marks that keep a status readable without its colour. Types only as
 * imports, so it runs under `node --test`.
 */
import type { BackupJob, BackupRun, WidgetCategory, WidgetStatus } from '#lib/api/types.js';

/** Columns of a widget on the 12-column dashboard grid. */
export type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;

/** Site, then category: the order an on-call person thinks in. */
export const CATEGORY_ORDER: readonly WidgetCategory[] = ['servers', 'storage', 'network', 'services', 'backups', 'other'];

/** Fill of a monitor cell, token based. */
export const STATUS_FILL: Record<WidgetStatus, string> = {
	up: 'var(--c-river-up)',
	degraded: 'var(--c-advisory)',
	down: 'var(--c-warning)',
	paused: 'var(--c-ink-3)'
};

/** The glyph drawn on a cell that is not up: colour never stands alone. */
export const STATUS_GLYPH: Record<WidgetStatus, string> = {
	up: '',
	degraded: '!',
	down: '×',
	paused: '‖'
};

const STATUS_RANK: Record<WidgetStatus, number> = { down: 0, degraded: 1, paused: 2, up: 3 };

export interface Counts {
	total: number;
	/** Not paused. */
	active: number;
	/** Answering: up or degraded. */
	up: number;
	down: number;
	degraded: number;
	paused: number;
}

export function countStatuses(monitors: readonly { status: WidgetStatus }[]): Counts {
	const c: Counts = { total: monitors.length, active: 0, up: 0, down: 0, degraded: 0, paused: 0 };
	for (const { status } of monitors) {
		if (status === 'paused') c.paused += 1;
		else {
			c.active += 1;
			if (status === 'down') c.down += 1;
			else c.up += 1;
			if (status === 'degraded') c.degraded += 1;
		}
	}
	return c;
}

/** One word for a group: something down, something degraded, all fine, or nothing watched. */
export type Health = 'down' | 'degraded' | 'healthy' | 'idle';

export function healthOf(c: Counts): Health {
	if (c.down > 0) return 'down';
	if (c.degraded > 0) return 'degraded';
	return c.active > 0 ? 'healthy' : 'idle';
}

function categoryIndex(category: string): number {
	const i = (CATEGORY_ORDER as readonly string[]).indexOf(category);
	return i < 0 ? CATEGORY_ORDER.length : i;
}

/** Stable sort by category, in `CATEGORY_ORDER`. */
export function sortByCategory<T extends { category: string }>(monitors: readonly T[]): T[] {
	return monitors
		.map((monitor, i) => ({ monitor, i }))
		.sort((a, b) => categoryIndex(a.monitor.category) - categoryIndex(b.monitor.category) || a.i - b.i)
		.map(({ monitor }) => monitor);
}

/** Non-empty categories of a site, in order, each with its monitors. */
export function groupByCategory<T extends { category: WidgetCategory }>(
	monitors: readonly T[]
): { category: WidgetCategory; monitors: T[] }[] {
	const groups = new Map<WidgetCategory, T[]>();
	for (const monitor of sortByCategory(monitors)) {
		const list = groups.get(monitor.category) ?? [];
		list.push(monitor);
		groups.set(monitor.category, list);
	}
	return [...groups].map(([category, list]) => ({ category, monitors: list }));
}

/** Categories present in at least one site: the matrix columns. */
export function usedCategories(sites: readonly { monitors: readonly { category: WidgetCategory }[] }[]): WidgetCategory[] {
	const seen = new Set<string>();
	for (const site of sites) for (const monitor of site.monitors) seen.add(monitor.category);
	return [...seen].sort((a, b) => categoryIndex(a) - categoryIndex(b)) as WidgetCategory[];
}

/** Worst first (down, degraded, paused, up), stable. */
export function worstFirst<T extends { status: WidgetStatus }>(list: readonly T[]): T[] {
	return list
		.map((item, i) => ({ item, i }))
		.sort((a, b) => STATUS_RANK[a.item.status] - STATUS_RANK[b.item.status] || a.i - b.i)
		.map(({ item }) => item);
}

/** Last finite value of a series, or null. */
export function lastValue(series: readonly (number | null)[] | null | undefined): number | null {
	if (!series) return null;
	for (let i = series.length - 1; i >= 0; i--) {
		const v = series[i];
		if (v !== null && Number.isFinite(v)) return v;
	}
	return null;
}

// --- Top consumers -------------------------------------------------------------

export type Load = 'critical' | 'high' | 'normal';

/** A percentage at 90 or more is critical, at 85 high; a rate (Mb/s) has no threshold. */
export function loadOf(value: number, unit: string): Load {
	if (unit !== '%') return 'normal';
	if (value >= 90) return 'critical';
	if (value >= 85) return 'high';
	return 'normal';
}

/** Width of the bar, 2–100 %: a percentage as is, a rate against the largest of the list. */
export function barWidth(value: number, unit: string, max: number): number {
	const pct = unit === '%' ? value : max > 0 ? (value / max) * 100 : 0;
	return Math.max(2, Math.min(100, Number.isFinite(pct) ? pct : 0));
}

// --- Backups -----------------------------------------------------------------

/** The last `n` runs, oldest first, padded at the start with `null` for runs that never happened. */
export function runCells(runs: readonly BackupRun[], n = 14): (boolean | null)[] {
	const last = runs.slice(-n).map((run) => run.ok);
	return [...Array(Math.max(0, n - last.length)).fill(null), ...last];
}

/** Jobs grouped by site, sites in the order they first appear. */
export function jobsBySite(jobs: readonly BackupJob[]): { site: string; jobs: BackupJob[] }[] {
	const groups = new Map<string, BackupJob[]>();
	for (const job of jobs) {
		const list = groups.get(job.site) ?? [];
		list.push(job);
		groups.set(job.site, list);
	}
	return [...groups].map(([site, list]) => ({ site, jobs: list }));
}

/** Jobs that need a look: failed or stale. */
export function jobsToCheck(jobs: readonly BackupJob[]): number {
	return jobs.filter((job) => job.status === 'failed' || job.status === 'stale').length;
}

// --- Services ----------------------------------------------------------------

export function checkSummary(checks: readonly (boolean | null)[]): { total: number; failed: number } {
	let total = 0;
	let failed = 0;
	for (const check of checks) {
		if (check === null) continue;
		total += 1;
		if (!check) failed += 1;
	}
	return { total, failed };
}

// --- Dates -------------------------------------------------------------------

/** Calendar days from `now` to `date`, in local time: 0 today, 1 tomorrow, -1 yesterday. */
export function dayDiff(date: Date, now: Date): number {
	const a = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
	const b = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
	return Math.round((a - b) / 86_400_000);
}
