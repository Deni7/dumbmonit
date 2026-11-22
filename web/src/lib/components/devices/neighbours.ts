/**
 * The monitor before and after another in a list order (the order of the
 * /targets list), for the "Previous / Next" links of a monitor's page. No
 * wrap-around: the first has no previous, the last no next.
 */
export function neighbours<T>(order: readonly T[], current: T): { prev: T | null; next: T | null } {
	const at = order.indexOf(current);
	if (at === -1) return { prev: null, next: null };
	return { prev: order[at - 1] ?? null, next: order[at + 1] ?? null };
}
