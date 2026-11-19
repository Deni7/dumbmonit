// What an alert row carries (rule line, recent changes, freshness). Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	operatorSymbol,
	recentTransitions,
	secondsSince,
	isStale,
	STALE_AFTER_SECS
} from '../src/lib/components/alerts/facts.ts';

const entry = (id, fingerprint, at, over = {}) => ({
	id,
	fingerprint,
	at,
	dismissed: false,
	from_phase: 'pending',
	to_phase: 'firing',
	...over
});

test('operatorSymbol reads comparisons the way people write them', () => {
	assert.equal(operatorSymbol('>'), '>');
	assert.equal(operatorSymbol('>='), '≥');
	assert.equal(operatorSymbol('<='), '≤');
	assert.equal(operatorSymbol('<'), '<');
});

test('recentTransitions keeps this alert and its folded siblings, newest first, capped', () => {
	const history = [
		entry(1, 'a', '2026-11-18T10:00:00Z'),
		entry(2, 'b', '2026-11-18T12:00:00Z'),
		entry(3, 'other', '2026-11-18T13:00:00Z'),
		entry(4, 'a', '2026-11-18T11:00:00Z'),
		entry(5, 'a', '2026-11-18T09:00:00Z'),
		entry(6, 'a', '2026-11-18T08:00:00Z')
	];
	assert.deepEqual(
		recentTransitions(history, ['a', 'b'], 3).map((e) => e.id),
		[2, 4, 1]
	);
});

test('recentTransitions skips hidden entries and does not mutate its input', () => {
	const history = [
		entry(1, 'a', '2026-11-18T10:00:00Z', { dismissed: true }),
		entry(2, 'a', '2026-11-18T09:00:00Z')
	];
	assert.deepEqual(recentTransitions(history, ['a']).map((e) => e.id), [2]);
	assert.equal(history[0].id, 1);
	assert.deepEqual(recentTransitions(history, []), []);
});

test('secondsSince is whole seconds, never negative, null when unknown', () => {
	const now = Date.parse('2026-11-18T10:00:30Z');
	assert.equal(secondsSince(new Date('2026-11-18T10:00:00Z'), now), 30);
	assert.equal(secondsSince(new Date('2026-11-18T10:05:00Z'), now), 0);
	assert.equal(secondsSince(null, now), null);
});

test('isStale only after several missed polls, and never before the first reading', () => {
	const now = 1_000_000;
	assert.equal(isStale(null, now), false);
	assert.equal(isStale(now - 30_000, now), false);
	assert.equal(isStale(now - (STALE_AFTER_SECS + 1) * 1000, now), true);
});
