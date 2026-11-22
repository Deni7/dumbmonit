// Password strength meter. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { passwordStrength, isCommonPassword } from '../src/lib/password-strength.ts';

test('common passwords score zero', () => {
	assert.equal(passwordStrength('password'), 0);
	assert.equal(passwordStrength('Password123'), 0);
	assert.equal(passwordStrength('P@ssw0rd'), 0);
	assert.equal(isCommonPassword('Welcome1'), true);
});

test('patterns and repeats score zero', () => {
	assert.equal(passwordStrength('aaaaaaaaaaaa'), 0);
	assert.equal(passwordStrength('abcdefghijkl'), 0);
	assert.equal(passwordStrength(''), 0);
});

test('length and variety raise the score', () => {
	assert.ok(passwordStrength('kx9fQzt') <= 1);
	assert.ok(passwordStrength('correct horse battery staple') >= 3);
	assert.equal(passwordStrength('Tr4vel-Bluebird-Lamp-Window9'), 4);
	assert.ok(passwordStrength('tqmzkrwvbnxy') >= passwordStrength('tqmzkrw'));
});
