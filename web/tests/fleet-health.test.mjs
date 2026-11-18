// The overview's fleet health: what counts as a problem, the tiles, the 24 hour bars. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TILE_LIMIT, dayAvailability, isProblem, outageSeconds, splitTiles, tileSize } from '../src/lib/components/enterprise/health.ts';

test('only down, degraded and misconfigured monitors are problems', () => {
	for (const status of ['down', 'degraded', 'config']) assert.equal(isProblem(status), true, status);
	for (const status of ['up', 'waiting', 'paused']) assert.equal(isProblem(status), false, status);
});

test('tiles shrink as the fleet grows, roomy enough to tap when it is small', () => {
	assert.equal(tileSize(10), 40);
	assert.equal(tileSize(36), 40);
	assert.equal(tileSize(37), 24);
	assert.equal(tileSize(100), 24);
	assert.equal(tileSize(500), 16);
});

test('tiles stop at a ceiling and count the rest', () => {
	const rows = Array.from({ length: 500 }, (_, i) => i);
	const { shown, hidden } = splitTiles(rows);
	assert.equal(shown.length, TILE_LIMIT);
	assert.equal(hidden, 500 - TILE_LIMIT);
	assert.deepEqual(splitTiles([1, 2, 3]), { shown: [1, 2, 3], hidden: 0 });
});

test('the outage is the run of down slots up to now, an unmeasured last slot skipped', () => {
	assert.equal(outageSeconds(['up', 'up', 'down', 'down', 'down'], 1200), 3600);
	assert.equal(outageSeconds(['up', 'down', 'down', 'none'], 1200), 2400);
	assert.equal(outageSeconds(['down', 'up', 'down', 'up'], 1200), 0);
	assert.equal(outageSeconds(['down', 'down', 'warn'], 1200), 0);
	assert.equal(outageSeconds(['none', 'none'], 1200), 0);
	assert.equal(outageSeconds([], 1200), 0);
});

test('day availability counts measured slots only', () => {
	assert.equal(dayAvailability(['up', 'up', 'down', 'up']), 75);
	assert.equal(dayAvailability(['none', 'up', 'none', 'warn']), 50);
	assert.equal(dayAvailability(['none', 'none']), null);
	assert.equal(dayAvailability([]), null);
});
