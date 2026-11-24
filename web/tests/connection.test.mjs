// Connection tracker. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createConnectionTracker } from '../src/lib/api/connection.ts';

test('lost after the threshold, restored by one success', () => {
	const seen = [];
	const t = createConnectionTracker(3, (lost) => seen.push(lost));
	t.failure();
	t.failure();
	assert.equal(t.lost, false);
	t.failure();
	assert.equal(t.lost, true);
	t.failure();
	t.success();
	assert.equal(t.lost, false);
	assert.deepEqual(seen, [true, false]);
});

test('a success in between resets the run', () => {
	const t = createConnectionTracker(3);
	t.failure();
	t.failure();
	t.success();
	t.failure();
	t.failure();
	assert.equal(t.lost, false);
});
