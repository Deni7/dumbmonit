/**
 * The columns of the monitors table, in one place so the header line and every
 * row share the very same grid, padding and row heights. The columns fold with
 * the width of the list itself (a container query set by `RackList`): narrow,
 * the status mark, the name (over its address and kind, with its alert count
 * and its sub-monitors on the name line) and the age of the last check; then
 * the site and the last 24 h; then the security grade. Hidden cells use the
 * same breakpoints (`SHOW`).
 *
 * The site column only exists when it can tell two monitors apart: with fewer
 * than two locations (or the list filtered to one), the grid has no site track
 * at all, header and rows alike (`gridFor`).
 */
import type { TargetState } from '#lib/format.js';

const GRID_WITH_SITE =
	'grid items-center gap-x-3 px-3 grid-cols-[1.25rem_minmax(0,1fr)_3rem] ' +
	'@xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_9rem_3rem] ' +
	'@3xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_11rem_2.75rem_3rem] ' +
	'@5xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_16rem_2.75rem_3rem]';

const GRID_WITHOUT_SITE =
	'grid items-center gap-x-3 px-3 grid-cols-[1.25rem_minmax(0,1fr)_3rem] ' +
	'@xl:grid-cols-[1.25rem_minmax(0,1fr)_9rem_3rem] ' +
	'@3xl:grid-cols-[1.25rem_minmax(0,1fr)_11rem_2.75rem_3rem] ' +
	'@5xl:grid-cols-[1.25rem_minmax(0,1fr)_16rem_2.75rem_3rem]';

/** The grid of the header line and of every row, with or without the site column. */
export function gridFor(site: boolean): string {
	return site ? GRID_WITH_SITE : GRID_WITHOUT_SITE;
}

/** Fixed heights, so the header and the rows never differ from one list to the next. */
export const HEADER_HEIGHT = 'h-9';
export const ROW_HEIGHT = 'h-14';

/** Cell visibility per column, matching `gridFor`. The site cell exists only with the site column. */
export const SHOW = {
	site: 'hidden @xl:flex',
	river: 'hidden @xl:flex',
	security: 'hidden @3xl:flex'
} as const;

/**
 * The monitor state, drawn as the same circle for every state, coloured by
 * its tone and marked inside so colour never stands alone: nothing when up,
 * a cross when down or offline, an exclamation mark when misconfigured, a dot
 * while pending, hollow when disabled or unknown.
 */
export type MarkGlyph = 'none' | 'cross' | 'bang' | 'dot' | 'ring';
export const STATE_GLYPH: Record<TargetState, MarkGlyph> = {
	online: 'none',
	offline: 'cross',
	down: 'cross',
	misconfigured: 'bang',
	pending: 'dot',
	disabled: 'ring',
	unknown: 'ring'
};
