/**
 * The visitor's timezone, written next to the dates of a public page: the
 * page is read from anywhere, and "14:05" means nothing without it. Pure, so
 * the tests can run it without the UI.
 */

/** Offset from UTC of `date` in the viewer's zone, as "UTC", "UTC+1", "UTC-3:30". */
export function utcOffset(date: Date): string {
	if (Number.isNaN(date.getTime())) return 'UTC';
	const minutes = -date.getTimezoneOffset();
	if (minutes === 0) return 'UTC';
	const sign = minutes > 0 ? '+' : '-';
	const abs = Math.abs(minutes);
	const hours = Math.floor(abs / 60);
	const rest = abs % 60;
	return `UTC${sign}${hours}${rest === 0 ? '' : `:${String(rest).padStart(2, '0')}`}`;
}

/** Name of the viewer's zone ("Europe/Paris"), for a tooltip; `''` when the browser does not say. */
export function zoneName(): string {
	try {
		return Intl.DateTimeFormat().resolvedOptions().timeZone ?? '';
	} catch {
		return '';
	}
}

/** Time of day with its zone: "14:05 UTC+1". */
export function clockWithZone(date: Date, locale?: string): string {
	const time = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date);
	return `${time} ${utcOffset(date)}`;
}

/** An already formatted date and the zone it is in: "Nov 3, 2026, 2:05 PM UTC+1". */
export function withZone(formatted: string, date: Date | null): string {
	return date ? `${formatted} ${utcOffset(date)}` : formatted;
}
