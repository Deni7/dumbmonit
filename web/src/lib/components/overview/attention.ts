/**
 * What the Overview keeps from the sky and the history: the "Needs
 * attention" rows and the incidents of the last days.
 *
 * Pure on purpose (types only), so it is tested with `node --test`; the
 * component picks the words.
 */
import type { SkyRow } from './sky.js';

export interface AttentionRow {
	row: SkyRow;
	/** Alerts held back behind this monitor while it is down (its guests). */
	held: number;
}

/**
 * Only what is broken now: monitors down, unreachable or misconfigured, and
 * firing alerts nobody has acknowledged or snoozed (info alerts stay on the
 * Alerts page). An alert held back behind a down parent is not a row of its
 * own: it is counted on the parent's row. Keeps the sky's reading order.
 */
export function attentionRows(rows: readonly SkyRow[]): AttentionRow[] {
	const held = new Map<number, number>();
	for (const row of rows) {
		if (row.kind !== 'alert' || row.alert.effective_phase !== 'suppressed' || !row.parent) continue;
		held.set(row.parent.id, (held.get(row.parent.id) ?? 0) + 1);
	}
	const out: AttentionRow[] = [];
	const counted = new Set<number>();
	for (const row of rows) {
		if (row.kind === 'alert') {
			const alert = row.alert;
			if (alert.effective_phase !== 'firing' || alert.acked || alert.silenced || alert.severity === 'info') continue;
		}
		const id = row.kind === 'device' ? row.target.id : (row.target?.id ?? null);
		let guests = 0;
		if (id !== null && !counted.has(id) && held.has(id)) {
			guests = held.get(id) ?? 0;
			counted.add(id);
		}
		out.push({ row, held: guests });
	}
	return out;
}

/** Monitors the rows are about, each once: "N monitors need attention". */
export function attentionMonitors(rows: readonly AttentionRow[]): number {
	const ids = new Set<string>();
	for (const { row } of rows) {
		if (row.kind === 'device') ids.add(`t${row.target.id}`);
		else ids.add(row.target ? `t${row.target.id}` : `a${row.alert.fingerprint}`);
	}
	return ids.size;
}

/** Server timestamps are UTC without a suffix (`parseServerDate` does the same). */
function serverTime(value: string): number {
	const normalised = /[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value.replace(' ', 'T') : `${value.replace(' ', 'T')}Z`;
	return new Date(normalised).getTime();
}

export interface IncidentDay {
	/** Local midnight that starts the day, in seconds. */
	day: number;
	/** Incidents that started this day. */
	count: number;
	/** Seconds of this day spent inside an incident. */
	seconds: number;
}

export interface IncidentWeek {
	/** The last `days` local days including today, oldest first. */
	days: IncidentDay[];
	count: number;
	seconds: number;
	/** The same two figures over the `days` days before. */
	previousCount: number;
	previousSeconds: number;
	/** The monitor behind the most incidents this week (`null` when none has a monitor). */
	noisiest: { target: number; count: number } | null;
}

interface HistoryRow {
	at: string;
	fingerprint: string;
	target_id: number | null;
	from_phase: string;
	to_phase: string;
}

/**
 * Incidents over the last `days` days: an incident runs from an alert going
 * `firing` to its `resolved` (still open: until `now`). Every firing counts, so
 * an alert that flaps three times is three incidents. Per local day: how many
 * started and how long incidents were open; as a total and over the `days` days
 * before for comparison; and the monitor behind the most incidents.
 */
export function incidentWeek(history: readonly HistoryRow[], now: number, days = 7): IncidentWeek {
	const today = new Date(now);
	today.setHours(0, 0, 0, 0);
	const starts: number[] = [];
	for (let i = days * 2 - 1; i >= 0; i -= 1) {
		const start = new Date(today);
		start.setDate(today.getDate() - i);
		starts.push(start.getTime());
	}
	const edges = [...starts, now];
	const windowStart = starts[0];
	const thisWeek = starts[days];

	const entries = history
		.map((entry) => ({ entry, at: serverTime(entry.at) }))
		.filter(({ at }) => Number.isFinite(at))
		.sort((a, b) => a.at - b.at);
	const open = new Map<string, { start: number; target: number | null }>();
	const incidents: { start: number; end: number; target: number | null }[] = [];
	for (const { entry, at } of entries) {
		const current = open.get(entry.fingerprint);
		if (entry.to_phase === 'firing') {
			if (!current) open.set(entry.fingerprint, { start: at, target: entry.target_id });
		} else if (current && (entry.to_phase === 'resolved' || entry.from_phase === 'firing')) {
			incidents.push({ start: current.start, end: at, target: current.target });
			open.delete(entry.fingerprint);
		}
	}
	for (const current of open.values()) incidents.push({ ...current, end: now });

	const out: IncidentDay[] = starts.map((start) => ({ day: Math.round(start / 1000), count: 0, seconds: 0 }));
	const perTarget = new Map<number, number>();
	for (const incident of incidents) {
		if (incident.end < windowStart || incident.start > now) continue;
		if (incident.start >= windowStart) {
			let index = starts.length - 1;
			while (index > 0 && incident.start < starts[index]) index -= 1;
			out[index].count += 1;
			if (incident.start >= thisWeek && incident.target !== null) {
				perTarget.set(incident.target, (perTarget.get(incident.target) ?? 0) + 1);
			}
		}
		for (let i = 0; i < starts.length; i += 1) {
			const overlap = Math.min(incident.end, edges[i + 1] ?? now) - Math.max(incident.start, edges[i]);
			if (overlap > 0) out[i].seconds += overlap / 1000;
		}
	}
	let noisiest: IncidentWeek['noisiest'] = null;
	for (const [target, count] of perTarget) {
		if (!noisiest || count > noisiest.count) noisiest = { target, count };
	}
	const sum = (list: IncidentDay[], key: 'count' | 'seconds') => list.reduce((total, day) => total + day[key], 0);
	const current = out.slice(days);
	const previous = out.slice(0, days);
	return {
		days: current,
		count: sum(current, 'count'),
		seconds: Math.round(sum(current, 'seconds')),
		previousCount: sum(previous, 'count'),
		previousSeconds: Math.round(sum(previous, 'seconds')),
		noisiest
	};
}
