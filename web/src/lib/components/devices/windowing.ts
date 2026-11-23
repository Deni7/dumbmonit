/**
 * Windowing of the compact monitors list (`RackList`): with thousands of
 * monitors, only the rows near the viewport are in the DOM, the others are
 * stood in for by padding above and below. Every compact row has the same
 * height (`ROW_REM`, enforced on the row while windowed), so where a row sits
 * is arithmetic, never a measurement. The page itself scrolls: the list's
 * position comes from its bounding box against the window.
 *
 * Pure, and loaded straight by Node (`npm test`).
 */

/** Height of one compact row, border included: the `ROW_INTRINSIC` of `columns.ts`. */
export const ROW_REM = 3.875;
/** Below this many rows in one list, every row is drawn: windowing would only add work. */
export const WINDOW_MIN_ROWS = 120;
/** Rows drawn past each edge of the viewport, so `j`/`k` and a fast scroll find them ready. */
export const OVERSCAN_ROWS = 12;

export interface WindowInput {
	/** Rows in the list. */
	count: number;
	/** One row, in pixels. */
	rowHeight: number;
	/** Top of the list relative to the top of the viewport (negative once scrolled past). */
	listTop: number;
	viewportHeight: number;
	overscan: number;
}

export interface WindowRange {
	/** First row drawn. */
	start: number;
	/** One past the last row drawn. */
	end: number;
	/** Room left for the rows above and below what is drawn, in pixels. */
	padTop: number;
	padBottom: number;
}

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** The rows to draw for a list at `listTop`, with `overscan` rows past each edge. */
export function visibleRange({ count, rowHeight, listTop, viewportHeight, overscan }: WindowInput): WindowRange {
	if (count <= 0 || !(rowHeight > 0)) return { start: 0, end: 0, padTop: 0, padBottom: 0 };
	const margin = Math.max(0, Math.floor(overscan));
	// Rows wholly above the top of the viewport, then the first row below its bottom.
	const above = Math.floor(-listTop / rowHeight);
	const below = Math.ceil((viewportHeight - listTop) / rowHeight);
	const start = clamp(above - margin, 0, count);
	const end = clamp(below + margin, start, count);
	return { start, end, padTop: start * rowHeight, padBottom: (count - end) * rowHeight };
}

/** Pixels of one row at this root font size (a viewer may have enlarged the text). */
export function rowHeightPx(rootFontPx: number): number {
	return ROW_REM * (rootFontPx > 0 ? rootFontPx : 16);
}

/** Windowing applies: the compact layout, a long list, and no reorder in progress (rows are hit-tested then). */
export function shouldWindow(count: number, compact: boolean, reordering: boolean): boolean {
	return compact && !reordering && count >= WINDOW_MIN_ROWS;
}
