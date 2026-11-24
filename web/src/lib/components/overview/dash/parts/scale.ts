/**
 * One scale for every panel of the overview, the alerts page's: a row is at
 * least 48px high with the same padding, a hairline above it; the main line
 * of a row is 14px semibold, what follows 13px in the second ink; a figure
 * or a time on the right is 13px with tabular digits. Icons come from
 * lucide, one size per role: 16px at the head of a row, 14px inside a line
 * of text (as in a plate). Monitor states use `StatusMark` (`STATUS_MARK` below).
 * Class strings only, so Tailwind sees them and `node --test` can load it.
 */
import type { WidgetStatus } from '#lib/api/types/overview.js';

/** A list row: hairline above, 48px at least, 16px sides. */
export const ROW = 'min-h-12 border-t border-line px-4 py-2';
/** The main line of a row (a name, a title). */
export const TEXT_MAIN = 'text-sm font-semibold text-ink';
/** A sentence that is the row's content (an event, a problem). */
export const TEXT_BODY = 'text-sm text-ink';
/** The line under the main one (kind, site, detail). */
export const TEXT_SUB = 'text-[0.8125rem] text-ink-2';
/** A figure or a time at the right of a row; the colour is the caller's (`text-ink-2` mostly). */
export const TEXT_SIDE = 'text-[0.8125rem] whitespace-nowrap tabular-nums';
/** The icon at the head of a row. */
export const ROW_ICON = 'size-4 shrink-0';
/** An icon inside a line of text. */
export const INLINE_ICON = 'size-3.5 shrink-0';
/** Padding of a panel's placeholder lines (reading, failed, empty). */
export const STATE_PAD = 'px-4 py-3';

/** A monitor's state as the shared `StatusMark` draws it: the glyph carries it as well as the colour. */
export const STATUS_MARK: Record<
	WidgetStatus,
	{ glyph: 'none' | 'cross' | 'bang' | 'ring'; tone: 'signal' | 'advisory' | 'warning' | 'ghost' }
> = {
	up: { glyph: 'none', tone: 'signal' },
	degraded: { glyph: 'bang', tone: 'advisory' },
	down: { glyph: 'cross', tone: 'warning' },
	paused: { glyph: 'ring', tone: 'ghost' }
};
