/**
 * The columns of the monitors table, in one place so the header line and every
 * row share the very same grid, padding and row heights. The columns fold with
 * the width of the list itself (a container query set by `RackList`): narrow,
 * the status mark, the name (over its kind and address, with its alert count
 * and its sub-monitors on the name line) and the age of the last check; then
 * the site and the last 24 h; then the security grade. Hidden cells use the
 * same breakpoints (`SHOW`).
 */
export const GRID =
	'grid items-center gap-x-3 px-3 grid-cols-[1.25rem_minmax(0,1fr)_3rem] ' +
	'@xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_9rem_3rem] ' +
	'@3xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_11rem_2.75rem_3rem] ' +
	'@5xl:grid-cols-[1.25rem_minmax(0,1fr)_2.5rem_16rem_2.75rem_3rem]';

/** Fixed heights, so the header and the rows never differ from one list to the next. */
export const HEADER_HEIGHT = 'h-9';
export const ROW_HEIGHT = 'h-14';

/** Cell visibility per column, matching `GRID`. */
export const SHOW = {
	site: 'hidden @xl:flex',
	river: 'hidden @xl:flex',
	security: 'hidden @3xl:flex'
} as const;

/** The monitor state, drawn as a shape (never colour alone). */
export type MarkShape = 'circle' | 'square' | 'triangle' | 'diamond' | 'ring';
export const STATE_SHAPE = {
	online: 'circle',
	offline: 'square',
	down: 'square',
	misconfigured: 'triangle',
	pending: 'diamond',
	disabled: 'ring',
	unknown: 'ring'
} as const satisfies Record<string, MarkShape>;
