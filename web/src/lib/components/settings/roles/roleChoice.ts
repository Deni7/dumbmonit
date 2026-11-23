/**
 * The role picker of Settings → Users offers the built-in roles and the custom
 * ones in one list: a custom role is the value `custom:<id>`, sent as the
 * viewer base role plus `custom_role_id`.
 */
import type { Role, User } from '#lib/api/types.js';

export type RoleChoice = Role | `custom:${number}`;

const PREFIX = 'custom:';

/** The picker value for an account. */
export function choiceOf(user: User): RoleChoice {
	return user.custom_role ? `custom:${user.custom_role.id}` : user.role;
}

/** True when the value picks a custom role. */
export function isCustomChoice(choice: RoleChoice): boolean {
	return choice.startsWith(PREFIX);
}

/** What the API expects for a picker value. */
export function rolePayload(choice: RoleChoice): { role: Role; custom_role_id?: number } {
	if (!isCustomChoice(choice)) return { role: choice as Role };
	const id = Number.parseInt(choice.slice(PREFIX.length), 10);
	return Number.isInteger(id) ? { role: 'viewer', custom_role_id: id } : { role: 'viewer' };
}
