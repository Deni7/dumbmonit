// The polling loop's scheduling: backoff on failures, wait left after a hidden tab. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_BACKOFF_MS, pollDelay, pollRemaining, startPolling } from '../src/lib/poll.ts';

/** Lets the loads in flight settle (setImmediate is not faked). */
const settle = () => new Promise((resolve) => setImmediate(resolve));

/** Advances the fake clock, then lets the load it started finish. */
async function advance(t, ms) {
	t.mock.timers.tick(ms);
	await settle();
}

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

test('a successful load runs at once, then every interval', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	const stop = startPolling(() => {
		calls += 1;
	}, 1000);
	await settle();
	assert.equal(calls, 1);
	await advance(t, 999);
	assert.equal(calls, 1);
	await advance(t, 1);
	assert.equal(calls, 2);
	await advance(t, 1000);
	assert.equal(calls, 3);
	stop();
});

test('a load that catches its error and returns false backs off', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	const stop = startPolling(async () => {
		calls += 1;
		return false;
	}, 1000);
	await settle();
	assert.equal(calls, 1);
	await advance(t, 1000);
	assert.equal(calls, 1, 'the second load waits twice the interval');
	await advance(t, 1000);
	assert.equal(calls, 2);
	await advance(t, 3999);
	assert.equal(calls, 2, 'the third waits four intervals');
	await advance(t, 1);
	assert.equal(calls, 3);
	stop();
});

test('a rejected load backs off too, and the first success restores the interval', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	let failing = true;
	const stop = startPolling(async () => {
		calls += 1;
		if (failing) throw new Error('down');
	}, 1000);
	await settle();
	assert.equal(calls, 1);
	failing = false;
	await advance(t, 2000);
	assert.equal(calls, 2);
	await advance(t, 1000);
	assert.equal(calls, 3);
	stop();
});

test('{ immediate: false } waits one interval before the first load', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	const stop = startPolling(
		() => {
			calls += 1;
		},
		1000,
		{ immediate: false }
	);
	await settle();
	assert.equal(calls, 0);
	await advance(t, 1000);
	assert.equal(calls, 1);
	stop();
});

test('two loads never overlap: the next one waits for the slow one to end', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	let finish;
	const stop = startPolling(() => {
		calls += 1;
		return new Promise((resolve) => (finish = resolve));
	}, 1000);
	await settle();
	await advance(t, 5000);
	assert.equal(calls, 1);
	finish();
	await settle();
	await advance(t, 1000);
	assert.equal(calls, 2);
	stop();
});

test('stopping aborts the load in flight and schedules nothing more', async (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	let calls = 0;
	let signal;
	const stop = startPolling((s) => {
		calls += 1;
		signal = s;
		return new Promise((_, reject) => s.addEventListener('abort', () => reject(new Error('aborted'))));
	}, 1000);
	await settle();
	stop();
	assert.equal(signal.aborted, true);
	await settle();
	await advance(t, 10_000);
	assert.equal(calls, 1);
});
