/**
 * Small figures shared by the dashboard widgets: durations in two parts,
 * deltas against the period before, error budgets, the geometry of the
 * mini bar charts and agent versions. Pure on purpose (types only), so it
 * runs under `node --test`; the components pick the words.
 */

export type DurationUnit = 's' | 'min' | 'h' | 'd';
export interface DurationPart {
	n: number;
	unit: DurationUnit;
}

/**
 * The two most significant parts of a duration: "40 s", "14 min",
 * "3 h 2 min", "2 d 4 h". A zero second part is dropped ("3 h", not "3 h 0 min").
 */
export function durationParts(seconds: number): DurationPart[] {
	const s = Math.max(0, Math.round(Number.isFinite(seconds) ? seconds : 0));
	if (s < 60) return [{ n: s, unit: 's' }];
	if (s < 3600) return [{ n: Math.floor(s / 60), unit: 'min' }];
	if (s < 86400) {
		const h = Math.floor(s / 3600);
		const min = Math.floor((s % 3600) / 60);
		return min > 0 ? [{ n: h, unit: 'h' }, { n: min, unit: 'min' }] : [{ n: h, unit: 'h' }];
	}
	const d = Math.floor(s / 86400);
	const h = Math.floor((s % 86400) / 3600);
	return h > 0 ? [{ n: d, unit: 'd' }, { n: h, unit: 'h' }] : [{ n: d, unit: 'd' }];
}

export interface Delta {
	/** current − previous. */
	diff: number;
	/** The change in percent of the previous value; null when the previous one is 0. */
	pct: number | null;
	direction: 'up' | 'down' | 'flat';
	/** Whether the change is good news; null when flat. */
	better: boolean | null;
}

/**
 * How a figure moved against the period before. `lowerIsBetter` for alerts,
 * MTTA and MTTR; `flatWithin` absorbs noise (|diff| ≤ it reads as flat).
 * Null when either side is unknown.
 */
export function deltaOf(
	current: number | null | undefined,
	previous: number | null | undefined,
	opts: { lowerIsBetter?: boolean; flatWithin?: number } = {}
): Delta | null {
	if (current === null || current === undefined || previous === null || previous === undefined) return null;
	if (!Number.isFinite(current) || !Number.isFinite(previous)) return null;
	const diff = current - previous;
	const pct = previous !== 0 ? (diff / previous) * 100 : null;
	const flat = Math.abs(diff) <= (opts.flatWithin ?? 0);
	const direction = flat ? 'flat' : diff > 0 ? 'up' : 'down';
	const better = flat ? null : opts.lowerIsBetter ? diff < 0 : diff > 0;
	return { diff, pct, direction, better };
}

/** Difference of two percentages in points, two decimals ("+0.03"); null when unknown. */
export function pointDelta(current: number | null | undefined, previous: number | null | undefined): number | null {
	const delta = deltaOf(current, previous);
	if (!delta) return null;
	return Math.round(delta.diff * 100) / 100;
}

/** "+0.03", "−0.12", "0.00": the sign always written, a true minus sign. */
export function signed(value: number, digits = 2): string {
	const fixed = Math.abs(value).toFixed(digits);
	if (Number(fixed) === 0) return fixed;
	return `${value > 0 ? '+' : '−'}${fixed}`;
}

/** Availability as the console writes it: two decimals, "100" only when it really is. */
export function availabilityText(value: number | null | undefined): string {
	if (value === null || value === undefined || !Number.isFinite(value)) return '—';
	if (value >= 99.9995) return '100';
	return Math.max(0, value).toFixed(2);
}

export type BudgetTone = 'ok' | 'low' | 'spent';

/** An error budget left (percent): spent at 0 or below, low under 25 %. */
export function budgetTone(left: number | null | undefined): BudgetTone | null {
	if (left === null || left === undefined || !Number.isFinite(left)) return null;
	if (left <= 0) return 'spent';
	if (left < 25) return 'low';
	return 'ok';
}

/** Width of a 0–100 bar, clamped. */
export function barWidth(pct: number | null | undefined): number {
	if (pct === null || pct === undefined || !Number.isFinite(pct)) return 0;
	return Math.max(0, Math.min(100, pct));
}

/**
 * How full the "days left" bar of an expiry is, on a `horizon`-day scale:
 * the closer, the fuller (never under 4 % so it stays visible). Null days
 * (an update, no date) draw nothing.
 */
export function daysLeftFill(days: number | null | undefined, horizon = 90): number {
	if (days === null || days === undefined || !Number.isFinite(days)) return 0;
	if (days <= 0) return 100;
	return Math.max(4, Math.min(100, 100 - (days / horizon) * 100));
}

export interface StackDay {
	critical: number;
	warning: number;
	info: number;
}
export interface StackRect {
	x: number;
	y: number;
	w: number;
	h: number;
	severity: 'critical' | 'warning' | 'info';
}

/**
 * Stacked bars, one per day (critical at the bottom, then warning, then
 * info), in a `width` × `height` box. A day without alerts gets a thin
 * baseline tick (`empty`) so the axis reads as "nothing", not as "no data".
 * The scale keeps a floor of `floor` alerts so one alert stays a short bar.
 */
export function stackBars(
	days: readonly StackDay[],
	opts: { width: number; height: number; gap?: number; floor?: number }
): { rects: StackRect[]; empty: { x: number; w: number }[]; peak: number } {
	const n = days.length;
	const rects: StackRect[] = [];
	const empty: { x: number; w: number }[] = [];
	if (n === 0) return { rects, empty, peak: 0 };
	const gap = opts.gap ?? 2;
	const slot = opts.width / n;
	const w = Math.max(1, slot - gap);
	const peak = Math.max(opts.floor ?? 4, ...days.map((d) => d.critical + d.warning + d.info));
	const unit = opts.height / peak;
	days.forEach((day, i) => {
		const x = i * slot + gap / 2;
		let y = opts.height;
		let any = false;
		for (const severity of ['critical', 'warning', 'info'] as const) {
			const count = day[severity];
			if (!(count > 0)) continue;
			any = true;
			const h = count * unit;
			y -= h;
			rects.push({ x, y, w, h, severity });
		}
		if (!any) empty.push({ x, w });
	});
	return { rects, empty, peak };
}

/** Numeric parts of a version ("0.20.1-rc.2" → [0, 20, 1]); unknown parts read as 0. */
function versionParts(version: string): number[] {
	return version
		.replace(/^v/i, '')
		.split(/[.+-]/)
		.slice(0, 3)
		.map((part) => {
			const n = Number.parseInt(part, 10);
			return Number.isFinite(n) ? n : 0;
		});
}

/** Newer first. */
export function compareVersions(a: string, b: string): number {
	const pa = versionParts(a);
	const pb = versionParts(b);
	for (let i = 0; i < 3; i += 1) {
		const diff = (pb[i] ?? 0) - (pa[i] ?? 0);
		if (diff !== 0) return diff;
	}
	return 0;
}

export interface VersionShare {
	version: string;
	count: number;
	/** `latest`: up to date; `behind`: same minor, older patch; `old`: an older minor or major. */
	rank: 'latest' | 'behind' | 'old';
}

/** Agent versions, newest first, each ranked against the latest one. */
export function versionShares(versions: readonly { version: string; count: number; latest: boolean }[]): VersionShare[] {
	const sorted = versions.filter((v) => v.count > 0).sort((a, b) => compareVersions(a.version, b.version));
	const latest = sorted.find((v) => v.latest)?.version ?? sorted[0]?.version ?? '';
	const [lmaj, lmin] = versionParts(latest);
	return sorted.map((v) => {
		if (v.latest || compareVersions(v.version, latest) <= 0) return { version: v.version, count: v.count, rank: 'latest' };
		const [maj, min] = versionParts(v.version);
		return { version: v.version, count: v.count, rank: maj === lmaj && min === lmin ? 'behind' : 'old' };
	});
}

export interface MonitorBarGroup {
	key: string;
	name: string;
	statuses: readonly string[];
}

/**
 * The "Monitors up" strip: one thin bar per monitor, grouped by site with
 * a wider gap between sites. A monitor down stands tall, a paused one short.
 * Returns the bars in viewBox units and, per site, its share of the width
 * for the labels under the strip.
 */
export function monitorBars(
	groups: readonly MonitorBarGroup[],
	opts: { height?: number; bar?: number; step?: number; siteGap?: number } = {}
): {
	bars: { x: number; y: number; w: number; h: number; status: string; group: string }[];
	width: number;
	labels: { key: string; name: string; share: number }[];
} {
	const height = opts.height ?? 26;
	const bar = opts.bar ?? 3.2;
	const step = opts.step ?? 4.6;
	const siteGap = opts.siteGap ?? 9;
	const bars: { x: number; y: number; w: number; h: number; status: string; group: string }[] = [];
	const labels: { key: string; name: string; share: number }[] = [];
	let x = 0;
	const shown = groups.filter((g) => g.statuses.length > 0);
	shown.forEach((group, gi) => {
		const start = x;
		for (const status of group.statuses) {
			const h = status === 'down' ? height : status === 'paused' ? height * 0.3 : height * 0.62;
			bars.push({ x, y: height - h, w: bar, h, status, group: group.key });
			x += step;
		}
		const end = x - (step - bar);
		labels.push({ key: group.key, name: group.name, share: end - start });
		if (gi < shown.length - 1) x = end + siteGap;
		else x = end;
	});
	return { bars, width: Math.max(x, 1), labels };
}
