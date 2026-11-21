// The "Last 24 h" bars: a device's silent hour is red, a probe's empty hour grey. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cellOf, riverCells, silenceMeansDown } from '../src/lib/components/enterprise/river.ts';

const device = (over) => ({ enabled: true, interval_secs: 60, ...over });

test('cells read the worst value of the slot', () => {
	assert.equal(cellOf(undefined), 'none');
	assert.equal(cellOf(1), 'up');
	assert.equal(cellOf(0), 'down');
	assert.equal(cellOf(0.5), 'warn');
});

test('a device checked often enough: silence means down', () => {
	assert.equal(silenceMeansDown(device({}), false, 3600), true);
	// Twice per slot at least: a 15 min check fits an hour, a 45 min one does not.
	assert.equal(silenceMeansDown(device({ interval_secs: 900 }), false, 3600), true);
	assert.equal(silenceMeansDown(device({ interval_secs: 2700 }), false, 3600), false);
});

test('a probe, a disabled device or an unknown interval keep grey', () => {
	assert.equal(silenceMeansDown(device({}), true, 3600), false);
	assert.equal(silenceMeansDown(device({ enabled: false }), false, 3600), false);
	assert.equal(silenceMeansDown(device({ interval_secs: 0 }), false, 3600), false);
});

test('a silent hour after the first measurement is down, before it nothing is known', () => {
	// Slots 1..6: nothing, nothing, up, nothing (unreachable), up, nothing (still unreachable).
	const bySlot = new Map([
		[3, 1],
		[5, 1]
	]);
	assert.deepEqual(riverCells(bySlot, 6, true), ['none', 'none', 'up', 'down', 'up', 'down']);
	assert.deepEqual(riverCells(bySlot, 6, false), ['none', 'none', 'up', 'none', 'up', 'none']);
});

test('measured slots keep their own value', () => {
	const bySlot = new Map([
		[1, 0],
		[2, 0.5],
		[3, 1]
	]);
	assert.deepEqual(riverCells(bySlot, 3, true), ['down', 'warn', 'up']);
});
