/**
 * Pure helpers of "Fleet by site" and "Services": counting, grouping and
 * ordering. Types only as imports, so it runs under `node --test`.
 */
import type { WidgetCategory, WidgetStatus } from '#lib/api/types.js';

/** Site, then category: the order an on-call person thinks in. */
export const CATEGORY_ORDER: readonly WidgetCategory[] = ['servers', 'storage', 'network', 'services', 'backups', 'other'];

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
