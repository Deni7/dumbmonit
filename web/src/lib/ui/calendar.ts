/**
 * Calendar arithmetic of the date pickers, on `YYYY-MM-DD` strings (the value
 * the forms exchange with the API): no time of day, no timezone, so a day
 * never shifts across midnight or a DST change. Weekdays follow
 * `Date#getUTCDay` (0 is Sunday). Pure: no DOM, no messages, so tests load it
 * directly.
 */

export interface DayParts {
	year: number;
	month: number;
	day: number;
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** The parts of a `YYYY-MM-DD` date, or `null` for anything else (31 February included). */
export function parseIsoDate(value: string | null | undefined): DayParts | null {
	const found = ISO_DATE.exec((value ?? '').trim());
	if (!found) return null;
	const [year, month, day] = [Number(found[1]), Number(found[2]), Number(found[3])];
	const date = new Date(Date.UTC(year, month - 1, day));
	if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return null;
	return { year, month, day };
}

const pad = (value: number, width = 2) => String(value).padStart(width, '0');

export function isoDate(year: number, month: number, day: number): string {
	const date = new Date(Date.UTC(year, month - 1, day));
	return `${pad(date.getUTCFullYear(), 4)}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`;
}

/** Today in the browser's own timezone (the day the person is living). */
export function todayIso(now: Date = new Date()): string {
	return isoDate(now.getFullYear(), now.getMonth() + 1, now.getDate());
}

/** A `Date` at noon UTC of the day: safe to format in any timezone with `timeZone: 'UTC'`. */
export function utcNoon(iso: string): Date {
	const parts = parseIsoDate(iso);
	return parts ? new Date(Date.UTC(parts.year, parts.month - 1, parts.day, 12)) : new Date(NaN);
}

export function addDays(iso: string, days: number): string {
	const parts = parseIsoDate(iso);
	return parts ? isoDate(parts.year, parts.month, parts.day + days) : iso;
}

/** Same day of another month, kept inside it (31 January + 1 month is 28 or 29 February). */
export function addMonths(iso: string, months: number): string {
	const parts = parseIsoDate(iso);
	if (!parts) return iso;
	const first = new Date(Date.UTC(parts.year, parts.month - 1 + months, 1));
	const last = new Date(Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0)).getUTCDate();
	return isoDate(first.getUTCFullYear(), first.getUTCMonth() + 1, Math.min(parts.day, last));
}

export function weekday(iso: string): number {
	return utcNoon(iso).getUTCDay();
}

export function firstOfMonth(iso: string): string {
	const parts = parseIsoDate(iso);
	return parts ? isoDate(parts.year, parts.month, 1) : iso;
}

export function lastOfMonth(iso: string): string {
	const parts = parseIsoDate(iso);
	return parts ? isoDate(parts.year, parts.month + 1, 0) : iso;
}

/** First day of the week holding `iso`. */
export function startOfWeek(iso: string, firstDay: number): string {
	return addDays(iso, -((weekday(iso) - firstDay + 7) % 7));
}

export function endOfWeek(iso: string, firstDay: number): string {
	return addDays(startOfWeek(iso, firstDay), 6);
}

/** Six weeks of seven days covering the month of `iso`: a grid whose height never changes. */
export function monthGrid(iso: string, firstDay: number): string[][] {
	const start = startOfWeek(firstOfMonth(iso), firstDay);
	return Array.from({ length: 6 }, (_, week) => Array.from({ length: 7 }, (_, day) => addDays(start, week * 7 + day)));
}

/** Weekday numbers in display order, from the first day of the week. */
export function weekdayOrder(firstDay: number): number[] {
	return Array.from({ length: 7 }, (_, i) => (firstDay + i) % 7);
}

/** `iso` kept between `min` and `max` (either may be empty). ISO dates compare as strings. */
export function clampIso(iso: string, min?: string | null, max?: string | null): string {
	if (min && iso < min) return min;
	if (max && iso > max) return max;
	return iso;
}

export function inRange(iso: string, min?: string | null, max?: string | null): boolean {
	return !(min && iso < min) && !(max && iso > max);
}

// Regions whose week starts on Sunday or Saturday (CLDR); every other one starts on Monday.
const SUNDAY_REGIONS = new Set(
	'AG AS BD BR BS BT BW BZ CA CN CO DM DO ET GT GU HK HN ID IL IN JM JP KE KH KR LA MH MM MO MT MX MZ NI NP PA PE PH PK PR PT PY SA SG SV TH TT TW UM US VE VI WS YE ZA ZW'.split(' ')
);
const SATURDAY_REGIONS = new Set('AE AF BH DJ DZ EG IQ IR JO KW LY OM QA SD SY'.split(' '));

/** First day of the week of a region (two-letter code): 0 Sunday, 1 Monday, 6 Saturday. */
export function firstDayOfRegion(region: string | undefined): number {
	const code = (region ?? '').toUpperCase();
	if (SUNDAY_REGIONS.has(code)) return 0;
	if (SATURDAY_REGIONS.has(code)) return 6;
	return 1;
}

type WeekInfoLocale = Intl.Locale & {
	getWeekInfo?: () => { firstDay: number };
	weekInfo?: { firstDay: number };
};

/** First day of the week for a locale (`fr` → Monday, `en-US` → Sunday), from `Intl` when it knows. */
export function firstDayOfWeek(locale: string): number {
	try {
		const parsed = new Intl.Locale(locale) as WeekInfoLocale;
		const info = typeof parsed.getWeekInfo === 'function' ? parsed.getWeekInfo() : parsed.weekInfo;
		// `Intl` counts 1 (Monday) to 7 (Sunday).
		if (info && Number.isInteger(info.firstDay)) return info.firstDay % 7;
		return firstDayOfRegion(parsed.maximize().region);
	} catch {
		return 1;
	}
}

export type RangePreset = 'last_7_days' | 'last_30_days' | 'this_month' | 'last_month' | 'last_quarter';

export const RANGE_PRESETS: RangePreset[] = ['last_7_days', 'last_30_days', 'this_month', 'last_month', 'last_quarter'];

/** The days a shortcut stands for, both included, as seen on `today`. */
export function presetRange(preset: RangePreset, today: string): { from: string; to: string } {
	switch (preset) {
		case 'last_7_days':
			return { from: addDays(today, -6), to: today };
		case 'last_30_days':
			return { from: addDays(today, -29), to: today };
		case 'this_month':
			return { from: firstOfMonth(today), to: today };
		case 'last_month': {
			const previous = addMonths(firstOfMonth(today), -1);
			return { from: previous, to: lastOfMonth(previous) };
		}
		case 'last_quarter': {
			const parts = parseIsoDate(today) ?? { year: 1970, month: 1, day: 1 };
			const quarterStart = isoDate(parts.year, Math.floor((parts.month - 1) / 3) * 3 + 1, 1);
			const from = addMonths(quarterStart, -3);
			return { from, to: addDays(quarterStart, -1) };
		}
	}
}

/** The shortcut a range matches exactly, if any. */
export function matchingPreset(from: string, to: string, today: string): RangePreset | null {
	return RANGE_PRESETS.find((preset) => {
		const range = presetRange(preset, today);
		return range.from === from && range.to === to;
	}) ?? null;
}

// ---- Time of day (`HH:mm`, 24 hours) ----------------------------------------

/**
 * A typed time read as `HH:mm`: `9`, `0930`, `9:30`, `9h30`, `9.30`, `14:30:15`
 * (seconds dropped), `2pm`, `2:30 PM`. `null` when it is not a time.
 */
export function parseTime(raw: string | null | undefined): string | null {
	const text = (raw ?? '').trim().toLowerCase().replace(/\s+/g, '');
	if (text === '') return null;
	const found = /^(\d{1,2})(?:[:h.]?(\d{2}))?(?::\d{2})?(am|pm|a|p)?$/.exec(text);
	if (!found) return null;
	let hours = Number(found[1]);
	const minutes = found[2] === undefined ? 0 : Number(found[2]);
	const suffix = found[3];
	if (suffix) {
		if (hours < 1 || hours > 12) return null;
		hours = (hours % 12) + (suffix.startsWith('p') ? 12 : 0);
	}
	if (hours > 23 || minutes > 59) return null;
	return `${pad(hours)}:${pad(minutes)}`;
}

/** `HH:mm` moved by `minutes`, round the clock. */
export function shiftTime(value: string, minutes: number): string {
	const parsed = parseTime(value) ?? '00:00';
	const [hours, mins] = parsed.split(':').map(Number);
	const total = (((hours * 60 + mins + minutes) % 1440) + 1440) % 1440;
	return `${pad(Math.floor(total / 60))}:${pad(total % 60)}`;
}

/** `YYYY-MM-DDTHH:mm` (the old `datetime-local` value) split into its day and its time. */
export function splitDateTime(value: string | null | undefined): { date: string; time: string } {
	const [date = '', time = ''] = (value ?? '').split('T');
	return { date: parseIsoDate(date) ? date : '', time: parseTime(time.slice(0, 5)) ?? '' };
}

/** The reverse: empty while the day is missing; a missing time is `fallback`. */
export function joinDateTime(date: string, time: string, fallback = '00:00'): string {
	if (!parseIsoDate(date)) return '';
	return `${date}T${parseTime(time) ?? fallback}`;
}
