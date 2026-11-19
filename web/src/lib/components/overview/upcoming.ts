/**
 * "Coming up" on the Overview, as plain functions: what is about to become a
 * problem, from the four places that know (the deadlines API: expiry dates,
 * disks that are filling up, late backups; the critical vulnerabilities; the
 * guided update), ordered by urgency. Types only as imports, so it runs under
 * `node --test`.
 */
import type { CveFinding, Deadline, UpdateInfo } from '#lib/api/types.js';

/** How far ahead the block looks, in days. */
export const HORIZON_DAYS = 30;

/** Rows drawn; the rest is behind "See all". */
export const ROW_LIMIT = 6;

export type UpcomingTone = 'urgent' | 'soon' | 'calm';

export interface UpcomingRow {
	key: string;
	/** A deadline kind (`certificate`, `domain`, `disk`, `backup`…), `vulnerability` or `update`. */
	kind: string;
	/** The object: host, mount point, backup job… Empty for a vulnerability and an update. */
	what: string;
	/** The monitor it sits on; null for the update. */
	monitor: string | null;
	/** Monitor id; null for the update. */
	target: number | null;
	/** Days until it happens, negative once it has; null for the update (no date). */
	days: number | null;
	/** `disk`: used, in percent; `backup`: age of the last success, in days. */
	value?: number;
	/** `vulnerability`: how many critical ones the monitor has. */
	count?: number;
	/** `vulnerability`: at least one is known to be exploited. */
	exploited?: boolean;
	/** `update`: the version on offer and the one running. */
	latest?: string;
	current?: string;
	/** When it falls due, UTC without suffix (`parseServerDate` adds the `Z`); empty without a date. */
	at: string;
	/** Path inside the app, without the base. */
	href: string;
	tone: UpcomingTone;
}

export function toneOf(days: number | null): UpcomingTone {
	if (days === null) return 'calm';
	if (days <= 1) return 'urgent';
	if (days <= 7) return 'soon';
	return 'calm';
}

/** Critical, still vulnerable, not ignored findings, one row per monitor. */
function vulnerabilityRows(findings: readonly CveFinding[]): UpcomingRow[] {
	const byTarget = new Map<number, { name: string; count: number; exploited: boolean }>();
	for (const finding of findings) {
		if (finding.severity !== 'critical' || finding.state !== 'vulnerable' || finding.ignored) continue;
		const entry = byTarget.get(finding.target_id) ?? { name: finding.target_name, count: 0, exploited: false };
		entry.count += 1;
		entry.exploited = entry.exploited || finding.exploited;
		byTarget.set(finding.target_id, entry);
	}
	return [...byTarget].map(([target, entry]) => ({
		key: `cve:${target}`,
		kind: 'vulnerability',
		what: '',
		monitor: entry.name,
		target,
		days: 0,
		at: '',
		count: entry.count,
		exploited: entry.exploited,
		href: `/targets/${target}`,
		tone: 'urgent'
	}));
}

/**
 * Everything that is due, most urgent first: what is already late or expired,
 * then by date; a vulnerability counts as due now; the update, which has no
 * date, comes last.
 */
export function upcomingRows(input: {
	deadlines: readonly Deadline[];
	findings: readonly CveFinding[];
	update: UpdateInfo | null;
	horizonDays?: number;
}): UpcomingRow[] {
	const horizon = input.horizonDays ?? HORIZON_DAYS;
	const rows: UpcomingRow[] = [];
	for (const d of input.deadlines) {
		if (d.days_left > horizon) continue;
		rows.push({
			key: `${d.kind}:${d.target}:${d.what}`,
			kind: d.kind,
			what: d.what,
			monitor: d.target_name,
			target: d.target,
			days: d.days_left,
			at: d.expires_at,
			value: d.value,
			href: `/targets/${d.target}`,
			tone: toneOf(d.days_left)
		});
	}
	rows.push(...vulnerabilityRows(input.findings));
	rows.sort((a, b) => (a.days ?? Infinity) - (b.days ?? Infinity) || a.key.localeCompare(b.key));
	const update = input.update;
	if (update?.update_available && update.latest) {
		rows.push({
			key: 'update',
			kind: 'update',
			what: '',
			monitor: null,
			target: null,
			days: null,
			at: '',
			latest: update.latest,
			current: update.current,
			href: '/settings/system#about',
			tone: 'calm'
		});
	}
	return rows;
}
