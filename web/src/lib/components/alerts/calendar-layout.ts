/**
 * Layout math of the maintenance calendar: which local days an occurrence
 * touches, the days a month or week view shows, and the API window to ask for.
 * Pure (no DOM, no messages, no `#lib` imports) so tests load it directly.
 * Days are local `YYYY-MM-DD` strings: the calendar is read in the browser's
 * own timezone, the one the operator lives in.
 */

export type OccurrenceState = 'active' | 'upcoming' | 'past';

/** One dated occurrence, as `GET /api/alerts/silences/occurrences` returns it. */
export interface Occurrence {
	silence_id: number;
	name: string;
	starts_at: string;
	ends_at: string;
	state: OccurrenceState;
	recurring: boolean;
	enabled: boolean;
}

/** The part of an occurrence that falls on one local day. */
export interface DaySegment {
	key: string;
	occurrence: Occurrence;
	day: string;
	/** Minutes since local midnight, 0…1440. */
	startMinute: number;
	endMinute: number;
	continuesBefore: boolean;
	continuesAfter: boolean;
}

export type CalendarView = 'month' | 'week';

/** How a chip is coloured and named: never the colour alone. */
export type ChipKind = 'active' | 'upcoming' | 'recurring' | 'past';

const pad = (value: number, width = 2) => String(value).padStart(width, '0');

function parts(iso: string): [number, number, number] {
	const [y, m, d] = iso.split('-').map(Number);
	return [y, m, d];
}

/** Local noon of a day: safe across DST changes when adding days. */
function noon(iso: string): Date {
	const [y, m, d] = parts(iso);
	return new Date(y, m - 1, d, 12);
}

/** Start of a local day as a real instant. */
export function dayStart(iso: string): Date {
	const [y, m, d] = parts(iso);
	return new Date(y, m - 1, d, 0, 0, 0, 0);
}

export function localDay(date: Date): string {
	return `${pad(date.getFullYear(), 4)}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function addDays(iso: string, count: number): string {
	const date = noon(iso);
	date.setDate(date.getDate() + count);
	return localDay(date);
}

/** Moves by whole months, clamping to the shorter month's last day. */
export function addMonths(iso: string, count: number): string {
	const [y, m, d] = parts(iso);
	const first = new Date(y, m - 1 + count, 1, 12);
	const last = new Date(first.getFullYear(), first.getMonth() + 1, 0, 12).getDate();
	return localDay(new Date(first.getFullYear(), first.getMonth(), Math.min(d, last), 12));
}

/** Weekday of a day, 0 = Sunday. */
export function weekdayOf(iso: string): number {
	return noon(iso).getDay();
}

export function startOfWeek(iso: string, firstDay: number): string {
	const back = (weekdayOf(iso) - firstDay + 7) % 7;
	return addDays(iso, -back);
}

/** The days a view shows, in order. A month view is whole weeks, padded on both sides. */
export function viewDays(view: CalendarView, anchor: string, firstDay: number): string[] {
	let first: string;
	let count: number;
	if (view === 'week') {
		first = startOfWeek(anchor, firstDay);
		count = 7;
	} else {
		const [y, m] = parts(anchor);
		const monthFirst = `${pad(y, 4)}-${pad(m)}-01`;
		const monthLast = localDay(new Date(y, m, 0, 12));
		first = startOfWeek(monthFirst, firstDay);
		const lastShown = addDays(startOfWeek(monthLast, firstDay), 6);
		count = Math.round((noon(lastShown).getTime() - noon(first).getTime()) / 86_400_000) + 1;
	}
	return Array.from({ length: count }, (_, i) => addDays(first, i));
}

/** Previous or next page of the view: a month or a week. */
export function shiftAnchor(view: CalendarView, anchor: string, direction: -1 | 1): string {
	return view === 'month' ? addMonths(anchor, direction) : addDays(anchor, 7 * direction);
}

/** The `from` / `to` instants (ISO, UTC) covering every day of the view. */
export function queryWindow(days: string[]): { from: string; to: string } {
	const first = days[0];
	const last = days[days.length - 1];
	return { from: dayStart(first).toISOString(), to: dayStart(addDays(last, 1)).toISOString() };
}

/** Splits every occurrence across the local days it touches, among `days`. */
export function segmentsByDay(
	occurrences: Occurrence[],
	days: string[]
): Map<string, DaySegment[]> {
	const out = new Map<string, DaySegment[]>(days.map((day) => [day, []]));
	for (const occurrence of occurrences) {
		const start = new Date(occurrence.starts_at);
		const end = new Date(occurrence.ends_at);
		if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) continue;
		for (const day of days) {
			const from = dayStart(day);
			const to = dayStart(addDays(day, 1));
			if (start >= to || end <= from) continue;
			const startMinute = start > from ? Math.round((start.getTime() - from.getTime()) / 60_000) : 0;
			const endMinute = end < to ? Math.round((end.getTime() - from.getTime()) / 60_000) : 1440;
			out.get(day)?.push({
				key: `${occurrence.silence_id}-${occurrence.starts_at}-${day}`,
				occurrence,
				day,
				startMinute,
				endMinute,
				continuesBefore: start < from,
				continuesAfter: end > to
			});
		}
	}
	for (const list of out.values()) {
		list.sort((a, b) => a.startMinute - b.startMinute || a.occurrence.silence_id - b.occurrence.silence_id);
	}
	return out;
}

export function chipKind(occurrence: Occurrence): ChipKind {
	if (occurrence.state === 'active') return 'active';
	if (occurrence.state === 'past') return 'past';
	return occurrence.recurring ? 'recurring' : 'upcoming';
}

/** "02:00" from minutes since midnight; 1440 reads as the end of the day. */
export function clockLabel(minute: number): string {
	const value = Math.min(minute, 1439);
	return `${pad(Math.floor(value / 60))}:${pad(value % 60)}`;
}

/** Day a click on an empty cell prefills: a default start time of 02:00, today excepted. */
export function prefillStart(day: string, today: string, now: Date = new Date()): string {
	if (day === today) {
		const rounded = new Date(now.getTime() + 3_600_000);
		rounded.setMinutes(0, 0, 0);
		if (localDay(rounded) === day) return `${day}T${pad(rounded.getHours())}:00`;
	}
	return `${day}T02:00`;
}

/** Roving-tabindex arrow moves of the grid: by a day or a week, or null for other keys. */
export function moveFocus(key: string, day: string): string | null {
	if (key === 'ArrowLeft') return addDays(day, -1);
	if (key === 'ArrowRight') return addDays(day, 1);
	if (key === 'ArrowUp') return addDays(day, -7);
	if (key === 'ArrowDown') return addDays(day, 7);
	if (key === 'PageUp') return addMonths(day, -1);
	if (key === 'PageDown') return addMonths(day, 1);
	return null;
}
