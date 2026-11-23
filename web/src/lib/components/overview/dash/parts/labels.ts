/**
 * Words of the site views: statuses, categories, a group's health, and the
 * dates of rows ("Today 22:00", "Wed 06:00", "Dec 2"). Kept apart from the
 * pure helpers, which stay free of Paraglide for `node --test`.
 */
import type { WidgetCategory, WidgetStatus } from '#lib/api/index.js';
import { parseServerDate } from '#lib/format.js';
import { m } from '#lib/paraglide/messages.js';
import { getLocale } from '#lib/paraglide/runtime.js';
import { displayPrefs } from '#lib/stores/prefs.svelte.js';
import type { Tone } from '#lib/ui/index.js';
import { dayDiff, type Health } from './sites.js';

export function statusWord(status: WidgetStatus): string {
	switch (status) {
		case 'up':
			return m.w9layout_status_up();
		case 'down':
			return m.w9layout_status_down();
		case 'degraded':
			return m.w9layout_status_degraded();
		case 'paused':
			return m.w9layout_status_paused();
	}
}

export const STATUS_TONE: Record<WidgetStatus, Tone> = {
	up: 'signal',
	down: 'warning',
	degraded: 'advisory',
	paused: 'muted'
};

export function categoryLabel(category: WidgetCategory | string): string {
	switch (category) {
		case 'servers':
			return m.w9layout_category_servers();
		case 'storage':
			return m.w9layout_category_storage();
		case 'network':
			return m.w9layout_category_network();
		case 'services':
			return m.w9layout_category_services();
		case 'backups':
			return m.w9layout_category_backups();
		default:
			return m.w9layout_category_other();
	}
}

export const HEALTH_TONE: Record<Health, Tone> = {
	down: 'warning',
	degraded: 'advisory',
	healthy: 'signal',
	idle: 'ghost'
};

/** "2 down", "Degraded", "Healthy", "Nothing watched". */
export function healthWord(health: Health, down: number): string {
	switch (health) {
		case 'down':
			return m.w9layout_health_down({ count: down });
		case 'degraded':
			return m.w9layout_health_degraded();
		case 'healthy':
			return m.w9layout_health_healthy();
		case 'idle':
			return m.w9layout_health_idle();
	}
}

/** A monitor's tooltip: "pve-02, Proxmox VE node: Down". */
export function monitorTitle(monitor: { name: string; kind: string; status: WidgetStatus }): string {
	return monitor.kind
		? m.w9layout_monitor_title({ name: monitor.name, kind: monitor.kind, status: statusWord(monitor.status) })
		: m.w9layout_monitor_title_bare({ name: monitor.name, status: statusWord(monitor.status) });
}

/** A number as the reader's locale writes it ("99,93" in French), at most `digits` decimals. */
export function num(value: number, digits = 0): string {
	try {
		return new Intl.NumberFormat(getLocale(), { maximumFractionDigits: digits }).format(value);
	} catch {
		return String(value);
	}
}

// --- Dates -------------------------------------------------------------------

function format(date: Date, options: Intl.DateTimeFormatOptions): string {
	try {
		return new Intl.DateTimeFormat(getLocale(), options).format(date);
	} catch {
		return date.toISOString().slice(0, 16).replace('T', ' ');
	}
}

export function clock(date: Date): string {
	return format(date, { hour: '2-digit', minute: '2-digit', hour12: displayPrefs.hour12 });
}

function weekday(date: Date): string {
	return format(date, { weekday: 'short' });
}

function shortDate(date: Date): string {
	return format(date, { day: 'numeric', month: 'short' });
}

/** The day of a row, without the time: "Today", "Tomorrow", "Wed 25", "Dec 2". */
export function dayLabel(date: Date, now: Date = new Date()): string {
	const diff = dayDiff(date, now);
	if (diff === 0) return m.w9layout_when_today_bare();
	if (diff === 1) return m.w9layout_when_tomorrow_bare();
	if (diff === -1) return m.w9layout_when_yesterday_bare();
	if (diff > 1 && diff < 7) return format(date, { weekday: 'short', day: 'numeric' });
	return shortDate(date);
}

/** When something starts: "Today 22:00", "Tomorrow 06:00", "Wed 06:00", "Dec 2". Empty without a date. */
export function futureWhen(value: string | null | undefined, now: Date = new Date()): string {
	const date = parseServerDate(value);
	if (!date) return '';
	const diff = dayDiff(date, now);
	const time = clock(date);
	if (diff <= 0) return m.w9layout_when_today({ time });
	if (diff === 1) return m.w9layout_when_tomorrow({ time });
	if (diff < 7) return m.w9layout_when_weekday({ day: weekday(date), time });
	return shortDate(date);
}

/** When something last happened: "02:14", "Yesterday 23:40", "Sun 03:00", "Nov 12". Empty without a date. */
export function pastWhen(value: string | null | undefined, now: Date = new Date()): string {
	const date = parseServerDate(value);
	if (!date) return '';
	const diff = dayDiff(date, now);
	const time = clock(date);
	if (diff >= 0) return time;
	if (diff === -1) return m.w9layout_when_yesterday({ time });
	if (diff > -7) return m.w9layout_when_weekday({ day: weekday(date), time });
	return shortDate(date);
}
