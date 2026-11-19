// Email server form (Settings → Notifications) and the email switch of the
// status page editor: payload, password rules, editor notice. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	defaultPort,
	emailSubscription,
	formFrom,
	mailServerPayload,
	parsePort,
	storedPasswordApplies
} from '../src/lib/components/settings/mailServer.ts';

const saved = {
	host: 'smtp.example.org',
	port: null,
	effective_port: 587,
	security: 'starttls',
	username: 'robot',
	has_password: true,
	from_address: 'status@example.org',
	from_name: 'Acme status',
	configured: true,
	locked_by_env: false
};

test('the usual port follows the encryption', () => {
	assert.equal(defaultPort('starttls'), 587);
	assert.equal(defaultPort('tls'), 465);
	assert.equal(defaultPort('none'), 25);
});

test('a typed port is checked, an empty one means the usual port', () => {
	assert.equal(parsePort(''), null);
	assert.equal(parsePort(' 2525 '), 2525);
	assert.equal(parsePort('0'), undefined);
	assert.equal(parsePort('70000'), undefined);
	assert.equal(parsePort('25a'), undefined);
});

test('the password is never pre-filled and omitted secrets keep it', () => {
	const form = formFrom(saved);
	assert.equal(form.password, '');
	assert.equal(form.port, '');
	const payload = mailServerPayload(form);
	assert.equal('secrets' in payload, false);
	assert.equal(payload.port, null);
	assert.equal(payload.from_name, 'Acme status');
});

test('a typed password replaces it, the clear switch sends empty secrets', () => {
	assert.deepEqual(mailServerPayload({ ...formFrom(saved), password: 'pw' }).secrets, { password: 'pw' });
	assert.deepEqual(mailServerPayload({ ...formFrom(saved), clearPassword: true }).secrets, {});
	const trimmed = mailServerPayload({ ...formFrom(saved), host: '  smtp.example.org ', port: '465' });
	assert.equal(trimmed.host, 'smtp.example.org');
	assert.equal(trimmed.port, 465);
});

test('the stored password only applies to the same host and username', () => {
	assert.equal(storedPasswordApplies(saved, formFrom(saved)), true);
	assert.equal(storedPasswordApplies(saved, { ...formFrom(saved), host: 'SMTP.example.org' }), true);
	assert.equal(storedPasswordApplies(saved, { ...formFrom(saved), host: 'smtp.evil.example' }), false);
	assert.equal(storedPasswordApplies(saved, { ...formFrom(saved), username: 'other' }), false);
	assert.equal(storedPasswordApplies({ ...saved, has_password: false }, formFrom(saved)), false);
	assert.equal(storedPasswordApplies({ ...saved, locked_by_env: true }, formFrom(saved)), false);
	assert.equal(storedPasswordApplies(null, formFrom(saved)), false);
});

test('the editor says how subscribers will be mailed', () => {
	assert.equal(emailSubscription(false, null, saved), 'off');
	assert.equal(emailSubscription(true, null, saved), 'server');
	assert.equal(emailSubscription(true, null, { configured: false }), 'missing');
	assert.equal(emailSubscription(true, 4, { configured: false }), 'channel', 'a chosen channel wins');
	assert.equal(emailSubscription(true, null, null), 'unknown');
});
