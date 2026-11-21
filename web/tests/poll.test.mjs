// The polling loop's scheduling: backoff on failures, wait left after a hidden tab. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_BACKOFF_MS, pollDelay, pollRemaining } from '../src/lib/poll.ts';

test('a healthy poll keeps its interval', () => {
	assert.equal(pollDelay(60_000, 0), 60_000);
});

test('failures double the wait up to five minutes', () => {
	assert.equal(pollDelay(30_000, 1), 60_000);
	assert.equal(pollDelay(30_000, 2), 120_000);
	assert.equal(pollDelay(30_000, 3), 240_000);
	assert.equal(pollDelay(30_000, 4), MAX_BACKOFF_MS);
	assert.equal(pollDelay(30_000, 200), MAX_BACKOFF_MS);
});

test('an interval already above the cap is never shortened', () => {
	assert.equal(pollDelay(600_000, 3), 600_000);
});

test('a tab shown again refreshes at once only when the last refresh is older than the interval', () => {
	assert.equal(pollRemaining(60_000, 0, 90_000), 0);
	assert.equal(pollRemaining(60_000, 50_000, 70_000), 40_000);
});
