// Search, filter and sort of Settings → Users.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filterUsers, sortUsers } from '../src/lib/components/settings/usersList.ts';

const u = (username, extra = {}) => ({
	username,
	display_name: '',
	role: 'viewer',
	totp_enabled: false,
	last_login_at: null,
	email: null,
	...extra
});

const users = [
	u('zoe', { role: 'admin', totp_enabled: true, last_login_at: '2026-11-20T10:00:00Z' }),
	u('marc', { display_name: 'Marc Éclair', role: 'operator', last_login_at: '2026-11-21 08:00:00' }),
	u('anna', { email: 'anna@example.org' }),
	u('bob', { role: 'admin', last_login_at: '2026-11-19T10:00:00Z' })
];
const names = (list) => list.map((user) => user.username);

test('search looks at username, display name and email, ignoring case and accents', () => {
	assert.deepEqual(names(filterUsers(users, 'ECLAIR', false)), ['marc']);
	assert.deepEqual(names(filterUsers(users, 'example.org', false)), ['anna']);
	assert.deepEqual(names(filterUsers(users, '  ', false)), ['zoe', 'marc', 'anna', 'bob']);
	assert.deepEqual(names(filterUsers(users, 'nobody', false)), []);
});

test('the "without 2FA" filter keeps only accounts without a second factor', () => {
	assert.deepEqual(names(filterUsers(users, '', true)), ['marc', 'anna', 'bob']);
	assert.deepEqual(names(filterUsers(users, 'zoe', true)), []);
});

test('sorting by name, role or last sign-in', () => {
	assert.deepEqual(names(sortUsers(users, 'name')), ['anna', 'bob', 'marc', 'zoe']);
	assert.deepEqual(names(sortUsers(users, 'role')), ['bob', 'zoe', 'marc', 'anna']);
	assert.deepEqual(names(sortUsers(users, 'last_login')), ['marc', 'zoe', 'bob', 'anna']);
	// The input is left as it was.
	assert.deepEqual(names(users), ['zoe', 'marc', 'anna', 'bob']);
});
