/**
 * "Expiries and forecasts": the rows of "Coming up" (`upcoming.ts`: deadlines,
 * critical vulnerabilities, the update) plus the disk forecasts of the
 * dashboards' top list that the deadlines do not already name. Pure on
 * purpose (types only), so it runs under `node --test`.
 */
import type { TopEntry } from '#lib/api/types/overview.js';
import type { UpcomingRow } from '../../upcoming.js';

function toneOf(days: number): UpcomingRow['tone'] {
	if (days <= 1) return 'urgent';
	if (days <= 7) return 'soon';
	return 'calm';
}

/**
 * Adds the disks forecast to fill within `horizon` days. A monitor that
 * already has a disk deadline keeps that one (the deadlines API knows the
 * mount point). The result is sorted again, soonest first, the update last.
 */
export function withDiskForecasts(rows: readonly UpcomingRow[], disks: readonly TopEntry[], horizon = 90): UpcomingRow[] {
	const known = new Set(rows.filter((r) => r.kind === 'disk' && r.target !== null).map((r) => r.target));
	const extra: UpcomingRow[] = [];
	for (const disk of disks) {
		const days = disk.forecast_days;
		if (days === null || days === undefined || !Number.isFinite(days) || days > horizon) continue;
		if (known.has(disk.target_id)) continue;
		known.add(disk.target_id);
		extra.push({
			key: `forecast:${disk.target_id}:${disk.label}`,
			kind: 'disk',
			what: disk.label,
			// The label already names the monitor ("syn-01 Volume 2"): the context line gives the site.
			monitor: disk.site || disk.label,
			target: disk.target_id,
			days,
			value: disk.value,
			at: '',
			href: `/targets/${disk.target_id}`,
			tone: toneOf(days)
		});
	}
	const all = [...rows, ...extra];
	const update = all.filter((r) => r.kind === 'update');
	const dated = all
		.filter((r) => r.kind !== 'update')
		.sort((a, b) => (a.days ?? Infinity) - (b.days ?? Infinity) || a.key.localeCompare(b.key));
	return [...dated, ...update];
}
