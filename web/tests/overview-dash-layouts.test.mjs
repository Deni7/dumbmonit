// Pure helpers of the overview's "Fleet by site" and "Services" panels: counting,
// grouping, ordering. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	checkSummary,
	countStatuses,
	dayDiff,
	groupByCategory,
	healthOf,
	lastValue,
	sortByCategory,
	worstFirst
} from '../src/lib/components/overview/dash/parts/sites.ts';

const mon = (id, status = 'up', category = 'servers', name = `m${id}`) => ({ id, name, kind: '', status, category });

test('counts: degraded answers, paused is not watched', () => {
	const c = countStatuses([mon(1), mon(2, 'degraded'), mon(3, 'down'), mon(4, 'paused')]);
	assert.deepEqual(c, { total: 4, active: 3, up: 2, down: 1, degraded: 1, paused: 1 });
	assert.equal(healthOf(c), 'down');
	assert.equal(healthOf(countStatuses([mon(1), mon(2, 'degraded')])), 'degraded');
	assert.equal(healthOf(countStatuses([mon(1)])), 'healthy');
	assert.equal(healthOf(countStatuses([mon(1, 'paused')])), 'idle');
	assert.equal(healthOf(countStatuses([])), 'idle');
});

test('monitors go in category order, stable inside a category', () => {
	const list = [mon(1, 'up', 'backups'), mon(2, 'up', 'servers'), mon(3, 'up', 'other'), mon(4, 'up', 'servers'), mon(5, 'up', 'network')];
	assert.deepEqual(
		sortByCategory(list).map((m) => m.id),
		[2, 4, 5, 1, 3]
	);
	assert.deepEqual(
		groupByCategory(list).map((g) => [g.category, g.monitors.length]),
		[
			['servers', 2],
			['network', 1],
			['backups', 1],
			['other', 1]
		]
	);
});

test('worst first keeps the order inside a status', () => {
	const list = [mon(1), mon(2, 'down'), mon(3, 'paused'), mon(4, 'degraded'), mon(5, 'down')];
	assert.deepEqual(
		worstFirst(list).map((m) => m.id),
		[2, 5, 4, 3, 1]
	);
});

test('last value skips gaps', () => {
	assert.equal(lastValue([1, 2, null]), 2);
	assert.equal(lastValue(null), null);
	assert.equal(lastValue([]), null);
});

test('check summary ignores missing data', () => {
	assert.deepEqual(checkSummary([true, false, null, true]), { total: 3, failed: 1 });
	assert.deepEqual(checkSummary([]), { total: 0, failed: 0 });
});

test('day difference counts calendar days in local time', () => {
	const now = new Date(2026, 10, 23, 23, 30);
	assert.equal(dayDiff(new Date(2026, 10, 23, 0, 5), now), 0);
	assert.equal(dayDiff(new Date(2026, 10, 24, 0, 5), now), 1);
	assert.equal(dayDiff(new Date(2026, 10, 22, 23, 59), now), -1);
	assert.equal(dayDiff(new Date(2026, 11, 2, 12), now), 9);
});
