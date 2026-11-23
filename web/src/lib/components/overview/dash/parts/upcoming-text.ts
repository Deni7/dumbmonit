/**
 * The words of an upcoming row (deadline, disk forecast, late backup,
 * vulnerability, update), the same as the classic Overview's "Coming up"
 * block, reusing its messages, for the dashboards' "Next 14 days".
 */
import { parseServerDate } from '#lib/format.js';
import { m } from '#lib/paraglide/messages.js';
import type { UpcomingRow } from '../../upcoming.js';

export function upcomingKind(row: UpcomingRow): string {
	switch (row.kind) {
		case 'certificate':
			return m.deadlines_kind_certificate();
		case 'domain':
			return m.deadlines_kind_domain();
		case 'credential':
			return m.deadlines_kind_credential();
		case 'product':
			return m.deadlines_kind_product();
		case 'key':
			return m.deadlines_kind_key();
		case 'license':
			return m.deadlines_kind_license();
		case 'disk':
			return m.overview_upcoming_kind_disk();
		case 'backup':
			return m.overview_upcoming_kind_backup();
		case 'vulnerability':
			return m.overview_upcoming_kind_vulnerability();
		case 'update':
			return m.overview_upcoming_kind_update();
		default:
			return row.kind;
	}
}

function dueDate(row: UpcomingRow): string {
	const at = parseServerDate(row.at);
	return at ? at.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }) : '';
}

export function upcomingTitle(row: UpcomingRow): string {
	const late = row.days !== null && row.days <= 0;
	switch (row.kind) {
		case 'disk':
			return late ? m.overview_upcoming_t_disk_full({ what: row.what }) : m.overview_upcoming_t_disk({ what: row.what });
		case 'backup':
			return late ? m.overview_upcoming_t_backup_late({ what: row.what }) : m.overview_upcoming_t_backup({ what: row.what });
		case 'vulnerability':
			return m.overview_upcoming_t_vuln({ count: row.count ?? 0 });
		case 'update':
			return m.overview_upcoming_t_update({ version: row.latest ?? '' });
		default:
			return late ? m.overview_upcoming_t_expired({ what: row.what }) : m.overview_upcoming_t_expires({ what: row.what });
	}
}

export function upcomingContext(row: UpcomingRow): string {
	const monitor = row.monitor ?? '';
	const late = row.days !== null && row.days <= 0;
	switch (row.kind) {
		case 'disk':
			return m.overview_upcoming_c_disk({ percent: Math.round(row.value ?? 0), monitor });
		case 'backup':
			return m.overview_upcoming_c_backup({ count: Math.max(1, Math.round(row.value ?? 0)), monitor });
		case 'vulnerability':
			return row.exploited ? m.overview_upcoming_c_vuln_exploited({ monitor }) : m.overview_upcoming_c_vuln({ monitor });
		case 'update':
			return m.overview_upcoming_c_update({ current: row.current ?? '' });
		default:
			return late
				? m.overview_upcoming_c_expired({ date: dueDate(row), monitor })
				: m.overview_upcoming_c_expires({ date: dueDate(row), monitor });
	}
}

/** "in 6 days", "~11 days" (a forecast), "3 days ago", "today", "now". */
export function upcomingDue(row: UpcomingRow): string {
	if (row.kind === 'update') return m.overview_upcoming_due_available();
	if (row.kind === 'vulnerability') return m.overview_upcoming_due_now();
	const days = row.days ?? 0;
	if (days <= -1) return m.overview_upcoming_due_ago({ count: Math.ceil(-days) });
	if (days < 1) return m.overview_upcoming_due_today();
	if (row.kind === 'disk') return m.overview_upcoming_due_about({ count: Math.round(days) });
	return m.overview_upcoming_due_in({ count: Math.floor(days) });
}
