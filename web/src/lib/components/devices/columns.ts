/**
 * The columns of the monitors table, in one place so the header line and every
 * row share the very same grid. The columns fold with the width of the list
 * itself (a container query set by `RackList`): narrow, Status, Name, Checked
 * and the alerts; then Site and the last 24 h; then Response and the security
 * grade; wide, the address as well. Hidden cells use the same breakpoints.
 */
export const GRID =
	'grid items-center gap-x-2 px-3 grid-cols-[5.5rem_minmax(0,1fr)_3rem_2.5rem] ' +
	'@xl:gap-x-3 @xl:grid-cols-[6.5rem_minmax(0,1fr)_2.5rem_7rem_3rem_2.5rem] ' +
	'@3xl:grid-cols-[6.5rem_minmax(0,1fr)_2.5rem_7rem_4.5rem_2.75rem_3rem_2.5rem] ' +
	'@5xl:grid-cols-[6.5rem_minmax(0,1.4fr)_minmax(0,1fr)_2.5rem_7rem_4.5rem_2.75rem_3rem_2.5rem]';

/** Cell visibility per column, matching `GRID`. */
export const SHOW = {
	address: 'hidden @5xl:block',
	site: 'hidden @xl:flex',
	river: 'hidden @xl:flex',
	response: 'hidden @3xl:block',
	security: 'hidden @3xl:flex'
} as const;
