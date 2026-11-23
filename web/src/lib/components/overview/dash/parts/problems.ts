/**
 * The rows of the dashboards' problems table, and what was resolved lately
 * (its calm state). Pure on purpose (types only), so it runs under
 * `node --test`.
 *
 * Unlike "Needs attention" on the classic overview, an acknowledged or
 * muted problem stays in the table, below the ones nobody has taken: the
 * table says who is on it.
 */
import type { SkyRow } from '../../sky.js';

export interface ProblemRow {
	row: SkyRow;
	/** Alerts held back behind this monitor while it is down (its guests). */
	held: number;
	/** Someone took it: the account (or null when unknown, a monitor acknowledged until it answers). */
	acked: { by: string | null } | null;
	/** A snoozed alert (a maintenance window or a mute covers it), not acknowledged. */
	muted: boolean;
}

/**
 * What is broken now: monitors down, unreachable or misconfigured, and
 * firing warning or critical alerts. Info alerts and alerts still building
 * up stay on the Alerts page; an alert held back behind a down parent is
 * counted on the parent's row. Rows nobody has taken come first, in the
 * sky's order (worst first); taken ones follow.
 *
 * `ackedTargets`: monitors acknowledged "until it answers again" (a recovery
 * silence), which "Needs attention" hides and this table keeps.
 */
export function problemRows(rows: readonly SkyRow[], ackedTargets: ReadonlySet<number> = new Set()): ProblemRow[] {
	const held = new Map<number, number>();
	for (const row of rows) {
		if (row.kind !== 'alert' || row.alert.effective_phase !== 'suppressed' || !row.parent) continue;
		held.set(row.parent.id, (held.get(row.parent.id) ?? 0) + 1);
	}
	const counted = new Set<number>();
	const open: ProblemRow[] = [];
	const taken: ProblemRow[] = [];
	for (const row of rows) {
		let acked: ProblemRow['acked'] = null;
		let muted = false;
		if (row.kind === 'alert') {
			const alert = row.alert;
			if (alert.effective_phase !== 'firing' || alert.severity === 'info') continue;
			if (alert.acked) acked = { by: alert.acked_by };
			else if (alert.silenced) muted = true;
		} else if (ackedTargets.has(row.target.id)) {
			acked = { by: null };
		}
		const id = row.kind === 'device' ? row.target.id : (row.target?.id ?? null);
		let guests = 0;
		if (id !== null && !counted.has(id) && held.has(id)) {
			guests = held.get(id) ?? 0;
			counted.add(id);
		}
		const out = { row, held: guests, acked, muted };
		(acked || muted ? taken : open).push(out);
	}
	return [...open, ...taken];
}

/** "token:ci" → "ci"; an account name stays as is. */
export function actorName(by: string | null | undefined): string | null {
	if (!by) return null;
	return by.startsWith('token:') ? by.slice('token:'.length) : by;
}

/** One or two letters for the round badge beside a name ("Sam" → "S", "alex.martin" → "AM"). */
export function initials(name: string): string {
	const parts = name
		.split(/[\s._@-]+/)
		.map((p) => p.trim())
		.filter(Boolean);
	if (parts.length === 0) return '?';
	const first = parts[0][0] ?? '';
	const second = parts.length > 1 ? (parts[1][0] ?? '') : '';
	return (first + second).toUpperCase();
}

interface HistoryRow {
	at: string;
	fingerprint: string;
	rule_uid: string;
	target_id: number | null;
	from_phase: string;
	to_phase: string;
	severity: string;
	notified: boolean;
}

export interface ResolvedEntry {
	fingerprint: string;
	rule_uid: string;
	target_id: number | null;
	severity: string;
	/** When it resolved (as the server wrote it). */
	at: string;
	/** How long it was firing, in seconds, when the start is in the history. */
	lasted: number | null;
}

/** Server timestamps are UTC, with or without a suffix. */
function serverTime(value: string): number {
	const normalised = /[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value.replace(' ', 'T') : `${value.replace(' ', 'T')}Z`;
	return new Date(normalised).getTime();
}

/**
 * The last alerts that resolved, newest first: what the calm state of the
 * problems table lists ("Recently resolved"). Info alerts are left out, as
 * they never were problems.
 */
export function recentResolved(history: readonly HistoryRow[], limit = 3): ResolvedEntry[] {
	const sorted = history
		.map((entry) => ({ entry, t: serverTime(entry.at) }))
		.filter(({ t }) => Number.isFinite(t))
		.sort((a, b) => a.t - b.t);
	const firingSince = new Map<string, number>();
	const out: ResolvedEntry[] = [];
	for (const { entry, t } of sorted) {
		if (entry.to_phase === 'firing') {
			if (!firingSince.has(entry.fingerprint)) firingSince.set(entry.fingerprint, t);
			continue;
		}
		if (entry.to_phase !== 'resolved') continue;
		const start = firingSince.get(entry.fingerprint);
		firingSince.delete(entry.fingerprint);
		// Resolved without having fired (it was only building up): nothing happened.
		if (start === undefined && entry.from_phase !== 'firing') continue;
		if (entry.severity === 'info') continue;
		out.push({
			fingerprint: entry.fingerprint,
			rule_uid: entry.rule_uid,
			target_id: entry.target_id,
			severity: entry.severity,
			at: entry.at,
			lasted: start === undefined ? null : Math.max(0, Math.round((t - start) / 1000))
		});
	}
	return out.reverse().slice(0, limit);
}
