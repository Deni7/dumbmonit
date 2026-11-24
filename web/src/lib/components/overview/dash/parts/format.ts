/**
 * Small figures of the overview: durations in two parts, availability as
 * the console writes it, and the geometry of the "Monitors up" strip. Pure
 * on purpose, so it runs under `node --test`; the components pick the words.
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

/** Availability as the console writes it: two decimals, "100" only when it really is. */
export function availabilityText(value: number | null | undefined): string {
	if (value === null || value === undefined || !Number.isFinite(value)) return '—';
	if (value >= 99.9995) return '100';
	return Math.max(0, value).toFixed(2);
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
