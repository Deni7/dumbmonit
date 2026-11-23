/**
 * The maintenance strip: the next `days` days, one row per site, each window
 * placed as a share of the strip. Built from the unrolled occurrences of the
 * maintenance windows (`/alerts/silences/occurrences`) and the windows
 * themselves (which monitor each covers). Pure on purpose (types only), so
 * it runs under `node --test`.
 */
import type { Silence, SilenceOccurrence } from '#lib/api/index.js';

export interface StripWindow {
	key: string;
	name: string;
	/** Left edge and width, in percent of the strip. */
	left: number;
	width: number;
	starts_at: string;
	ends_at: string;
	active: boolean;
}

export interface StripRow {
	key: string;
	name: string;
	windows: StripWindow[];
}

interface SiteLike {
	key: string;
	name: string;
	monitors: readonly { id: number }[];
}

function serverTime(value: string): number {
	const normalised = /[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value.replace(' ', 'T') : `${value.replace(' ', 'T')}Z`;
	return new Date(normalised).getTime();
}

/** The narrowest a window is drawn, in percent, so a 15-minute window stays visible. */
const MIN_WIDTH = 0.9;

/**
 * `from`: the start of the strip (local midnight today), `now`: the present,
 * both in ms. A window on no monitor in particular (every monitor) shows on
 * every row; a window on a monitor no site knows goes to the first row.
 * Without any site there is one row, keyed `all`, named `fallback`.
 */
export function maintenanceStrip(input: {
	occurrences: readonly SilenceOccurrence[];
	silences: readonly Silence[];
	sites: readonly SiteLike[];
	from: number;
	now: number;
	days?: number;
	fallback?: string;
}): { rows: StripRow[]; nowPct: number } {
	const days = input.days ?? 7;
	const span = days * 86400_000;
	const to = input.from + span;
	const rows: StripRow[] =
		input.sites.length > 0
			? input.sites.map((site) => ({ key: site.key, name: site.name, windows: [] }))
			: [{ key: 'all', name: input.fallback ?? '', windows: [] }];
	const siteOf = new Map<number, number>();
	input.sites.forEach((site, i) => site.monitors.forEach((m) => siteOf.set(m.id, i)));
	const silenceById = new Map(input.silences.map((s) => [s.id, s]));

	for (const occ of input.occurrences) {
		if (!occ.enabled) continue;
		const start = serverTime(occ.starts_at);
		const end = serverTime(occ.ends_at);
		if (!Number.isFinite(start) || !Number.isFinite(end) || end <= input.from || start >= to) continue;
		const left = ((Math.max(start, input.from) - input.from) / span) * 100;
		const width = Math.max(MIN_WIDTH, ((Math.min(end, to) - Math.max(start, input.from)) / span) * 100);
		const window: Omit<StripWindow, 'key'> = {
			name: occ.name,
			left: Math.min(left, 100 - MIN_WIDTH),
			width: Math.min(width, 100 - Math.min(left, 100 - MIN_WIDTH)),
			starts_at: occ.starts_at,
			ends_at: occ.ends_at,
			active: occ.state === 'active' || (start <= input.now && input.now < end)
		};
		const target = silenceById.get(occ.silence_id)?.target_id ?? null;
		const key = `${occ.silence_id}:${occ.starts_at}`;
		if (target === null) {
			rows.forEach((row) => row.windows.push({ ...window, key }));
		} else {
			rows[siteOf.get(target) ?? 0].windows.push({ ...window, key });
		}
	}
	for (const row of rows) row.windows.sort((a, b) => a.left - b.left);
	const nowPct = Math.max(0, Math.min(100, ((input.now - input.from) / span) * 100));
	return { rows, nowPct };
}

/** Occurrences still to come or running, soonest first (the list under the strip). */
export function nextOccurrences(occurrences: readonly SilenceOccurrence[], now: number, limit = 3): SilenceOccurrence[] {
	return occurrences
		.filter((o) => o.enabled && o.state !== 'past' && serverTime(o.ends_at) > now)
		.sort((a, b) => serverTime(a.starts_at) - serverTime(b.starts_at))
		.slice(0, limit);
}
