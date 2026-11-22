/**
 * Search, filter and sort of the Settings → Users list. Pure helpers, loaded
 * straight by Node in `npm test` (`tests/users-list.test.mjs`).
 */
import type { User } from '#lib/api/index.js';

export type UserSort = 'name' | 'role' | 'last_login';
export const USER_SORTS: readonly UserSort[] = ['name', 'role', 'last_login'];

/** The fields the helpers read: a full `User` qualifies. */
type Row = Pick<User, 'username' | 'display_name' | 'role' | 'totp_enabled' | 'last_login_at'> & {
	email?: string | null;
};

const ROLE_RANK: Record<string, number> = { admin: 0, operator: 1, viewer: 2 };

/** Name shown for an account: its display name, else its username. */
export function shownName(user: Row): string {
	return user.display_name.trim() || user.username;
}

/**
 * Accounts whose username, display name or email contains `query` (any case,
 * any accent), and, with `without2fa`, only those without a second factor.
 */
export function filterUsers<T extends Row>(users: readonly T[], query: string, without2fa: boolean): T[] {
	const needle = fold(query.trim());
	return users.filter((user) => {
		if (without2fa && user.totp_enabled) return false;
		if (!needle) return true;
		return [user.username, user.display_name, user.email ?? ''].some((field) => fold(field).includes(needle));
	});
}

/**
 * `name`: alphabetical. `role`: admins, then operators, then viewers, by name.
 * `last_login`: most recent sign-in first, never signed in last.
 */
export function sortUsers<T extends Row>(users: readonly T[], sort: UserSort): T[] {
	const byName = (a: T, b: T) => shownName(a).localeCompare(shownName(b), undefined, { sensitivity: 'base' });
	const copy = [...users];
	if (sort === 'role') {
		return copy.sort((a, b) => (ROLE_RANK[a.role] ?? 9) - (ROLE_RANK[b.role] ?? 9) || byName(a, b));
	}
	if (sort === 'last_login') {
		return copy.sort((a, b) => {
			const at = a.last_login_at ? Date.parse(withZone(a.last_login_at)) : Number.NaN;
			const bt = b.last_login_at ? Date.parse(withZone(b.last_login_at)) : Number.NaN;
			const aKnown = Number.isFinite(at);
			const bKnown = Number.isFinite(bt);
			if (aKnown && bKnown && at !== bt) return bt - at;
			if (aKnown !== bKnown) return aKnown ? -1 : 1;
			return byName(a, b);
		});
	}
	return copy.sort(byName);
}

/** Server timestamps are UTC, sometimes without a suffix. */
function withZone(value: string): string {
	return /[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value.replace(' ', 'T') : `${value.replace(' ', 'T')}Z`;
}

function fold(value: string): string {
	return value.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}
