/**
 * The words of the dashboard widgets that several of them share: status
 * words, two-part durations, clock times and the lines of an upcoming row
 * (the same sentences as "Coming up" on the classic overview).
 */
import { parseServerDate } from '#lib/format.js';
import { m } from '#lib/paraglide/messages.js';
import type { UpcomingRow } from '../../upcoming.js';
import { durationParts, type DurationUnit } from './format.js';
import type { DashStatus } from './plates.js';

const UNIT: Record<DurationUnit, (input: { n: number }) => string> = {
	s: (input) => m.misc_dur_s(input),
	min: (input) => m.misc_dur_min(input),
	h: (input) => m.misc_dur_h(input),
	d: (input) => m.misc_dur_d(input)
};

/** "40s", "14min", "3h 2min", "2d 4h". */
export function durationText(seconds: number): string {
	return durationParts(seconds)
		.map((part) => UNIT[part.unit]({ n: part.n }))
		.join(' ');
}

export function statusWord(status: DashStatus): string {
	switch (status) {
		case 'up':
			return m.w9dash_status_up();
		case 'degraded':
			return m.w9dash_status_degraded();
		case 'down':
			return m.misc_state_down();
		case 'paused':
			return m.w9dash_status_paused();
		case 'config':
			return m.misc_state_misconfigured();
		default:
			return m.misc_state_pending();
	}
}

function sameDay(a: Date, b: Date): boolean {
	return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/**
 * When something happened, as short as it can be read: "10:41" today,
 * "Sun 09:58" within the week, "Nov 12" before.
 */
export function clockText(value: string | Date | null | undefined, now = new Date()): string {
	const date = value instanceof Date ? value : parseServerDate(value);
	if (!date) return '';
	const time = date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	if (sameDay(date, now)) return time;
	if (now.getTime() - date.getTime() < 6 * 86400_000 && now.getTime() >= date.getTime()) {
		return `${date.toLocaleDateString(undefined, { weekday: 'short' })} ${time}`;
	}
	return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/**
 * When something will happen, as short as it can be read: "Today 22:00",
 * "Wed 06:00" within the week, "Dec 2" after.
 */
export function soonText(value: string | Date | null | undefined, now = new Date()): string {
	const date = value instanceof Date ? value : parseServerDate(value);
	if (!date) return '';
	const time = date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	if (sameDay(date, now)) return m.w9dash_common_today_at({ time });
	if (date.getTime() > now.getTime() && date.getTime() - now.getTime() < 6 * 86400_000) {
		return `${date.toLocaleDateString(undefined, { weekday: 'short' })} ${time}`;
	}
	return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** A day of a daily series (`YYYY-MM-DD`, UTC) as "Nov 10". */
export function dayText(day: string): string {
	const date = new Date(`${day}T12:00:00Z`);
	if (Number.isNaN(date.getTime())) return day;
	return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

// --- Upcoming rows (the same sentences as `ComingUp.svelte`) ------------------

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

/** "in 6 days", "~11 days" (a forecast), "3 days ago", "today", "now", "available". */
export function upcomingDue(row: UpcomingRow): string {
	if (row.kind === 'update') return m.overview_upcoming_due_available();
	if (row.kind === 'vulnerability') return m.overview_upcoming_due_now();
	const days = row.days ?? 0;
	if (days <= -1) return m.overview_upcoming_due_ago({ count: Math.ceil(-days) });
	if (days < 1) return m.overview_upcoming_due_today();
	if (row.kind === 'disk') return m.overview_upcoming_due_about({ count: Math.round(days) });
	return m.overview_upcoming_due_in({ count: Math.floor(days) });
}
