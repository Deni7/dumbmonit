// Free check interval of the monitor form.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	MAX_INTERVAL_SECS,
	MIN_INTERVAL_SECS,
	intervalProblem,
	intervalSeconds,
	splitInterval
} from '../src/lib/components/device-form/interval.ts';

test('splits into the largest unit that divides evenly', () => {
	assert.deepEqual(splitInterval(7200), { value: 2, unit: 'h' });
	assert.deepEqual(splitInterval(86400), { value: 1, unit: 'd' });
	assert.deepEqual(splitInterval(90), { value: 90, unit: 's' });
	assert.deepEqual(splitInterval(600), { value: 10, unit: 'm' });
});

test('converts a typed value and unit to seconds', () => {
	assert.equal(intervalSeconds('5', 'm'), 300);
	assert.equal(intervalSeconds(2, 'h'), 7200);
	assert.equal(intervalSeconds(' 45 ', 's'), 45);
	assert.equal(intervalSeconds('1.5', 'm'), null);
	assert.equal(intervalSeconds('', 's'), null);
	assert.equal(intervalSeconds('0', 'm'), null);
	assert.equal(intervalSeconds('-3', 's'), null);
});

test('bounds match the server (10 s to 7 days)', () => {
	assert.equal(MIN_INTERVAL_SECS, 10);
	assert.equal(MAX_INTERVAL_SECS, 604800);
	assert.equal(intervalProblem(null), 'invalid');
	assert.equal(intervalProblem(9), 'min');
	assert.equal(intervalProblem(10), null);
	assert.equal(intervalProblem(604800), null);
	assert.equal(intervalProblem(604801), 'max');
});
