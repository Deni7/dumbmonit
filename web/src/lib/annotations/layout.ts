/**
 * Where a change marker sits over a chart's plot. Pure functions, no imports:
 * the chart feeds them its visible time range and the plot width, whatever
 * unit that width is in (canvas or CSS pixels), and tests run them directly.
 */

/** A marker's place on the plot, `x` counted from the plot's left edge. */
export interface MarkerSpan {
	/** Left edge of the marker (of its window, clipped to the plot). */
	x: number;
	/** 0 for a single instant; the shaded width for a window. */
	width: number;
	/** The window began before the visible range: its left edge is the plot's. */
	clippedStart: boolean;
	/** The window ends after the visible range: its right edge is the plot's. */
	clippedEnd: boolean;
}

/**
 * Pixel span of a marker at `at` (seconds), or over `[at, until]`, on a plot that
 * shows `[min, max]` (seconds) across `plotWidth`. `null` when it is entirely
 * outside the range, or when the range or the plot has no extent. An instant
 * exactly on an edge is inside.
 */
export function markerSpan(
	at: number,
	until: number | undefined,
	min: number,
	max: number,
	plotWidth: number
): MarkerSpan | null {
	if (![at, min, max, plotWidth].every(Number.isFinite) || max <= min || plotWidth <= 0) return null;
	const end = until !== undefined && Number.isFinite(until) && until > at ? until : at;
	if (end < min || at > max) return null;
	const scale = plotWidth / (max - min);
	const start = Math.max(at, min);
	const stop = Math.min(end, max);
	return {
		x: (start - min) * scale,
		width: end > at ? (stop - start) * scale : 0,
		clippedStart: at < min,
		clippedEnd: end > max
	};
}

/** The window to ask the server for: the chart's visible range, in seconds. */
export function visibleRange(nowSeconds: number, rangeSeconds: number): { from: number; to: number } {
	return { from: nowSeconds - rangeSeconds, to: nowSeconds };
}

/**
 * Round a range to a step so that a poll that moved the window by a few seconds
 * does not look like a new window (and refetch). Both ends snap outward.
 */
export function snapRange(from: number, to: number, step = 60): { from: number; to: number } {
	return { from: Math.floor(from / step) * step, to: Math.ceil(to / step) * step };
}
