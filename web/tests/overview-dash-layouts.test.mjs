// Pure helpers of the overview dashboards' site views (honeycomb, sunburst,
// matrix, site cards, top consumers, backups, services). Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	SUN,
	SUN_LABELS,
	TAU,
	arcPath,
	clip,
	hexColumns,
	hexGrid,
	spread,
	sunburstLayout
} from '../src/lib/components/overview/dash/parts/geometry.ts';
import {
	barWidth,
	checkSummary,
	countStatuses,
	dayDiff,
	groupByCategory,
	healthOf,
	jobsBySite,
	jobsToCheck,
	lastValue,
	loadOf,
	runCells,
	sortByCategory,
	usedCategories,
	worstFirst
} from '../src/lib/components/overview/dash/parts/sites.ts';

const mon = (id, status = 'up', category = 'servers', name = `m${id}`) => ({ id, name, kind: '', status, category });

test('hex columns stay between 5 and 14 and never exceed the cells', () => {
	assert.equal(hexColumns(0), 1);
	assert.equal(hexColumns(1), 1);
	assert.equal(hexColumns(3), 3);
	assert.equal(hexColumns(26), 8);
	assert.equal(hexColumns(500), 14);
});

test('hex grid places one cell per monitor, odd rows shifted', () => {
	const grid = hexGrid(10, 4);
	assert.equal(grid.cells.length, 10);
	assert.equal(grid.cols, 4);
	assert.equal(grid.cells[0].cy, grid.cells[3].cy);
	assert.ok(grid.cells[4].cy > grid.cells[0].cy);
	assert.ok(grid.cells[4].cx > grid.cells[0].cx, 'second row starts half a cell in');
	for (const cell of grid.cells) {
		assert.ok(cell.cx > 0 && cell.cx < grid.width);
		assert.ok(cell.cy > 0 && cell.cy < grid.height);
		assert.equal(cell.points.split(' ').length, 6);
	}
	assert.deepEqual(hexGrid(0).cells, []);
});

test('an arc path is empty when reversed and a full turn is drawn in two halves', () => {
	assert.equal(arcPath(10, 20, 1, 1), '');
	assert.equal(arcPath(10, 20, 2, 1), '');
	const full = arcPath(10, 20, 0, TAU);
	assert.equal(full.match(/M/g).length, 2);
	assert.ok(!full.includes('NaN'));
	assert.match(arcPath(10, 20, 0, 4), /A20 20 0 1 1/, 'large arc flag past half a turn');
});

test('sunburst gives each monitor the same angle and sites add up', () => {
	const layout = sunburstLayout([
		{ key: 'a', name: 'Lyon', monitors: [mon(1), mon(2), mon(3, 'down', 'storage')] },
		{ key: 'b', name: 'Annecy', monitors: [mon(4, 'degraded', 'network')] },
		{ key: 'c', name: 'Empty', monitors: [] }
	]);
	assert.equal(layout.total, 4);
	assert.equal(layout.sites.length, 2, 'an empty site has no ring');
	assert.equal(layout.monitors.length, 4);
	assert.equal(layout.categories.length, 3);
	const [lyon, annecy] = layout.sites;
	assert.ok(Math.abs(lyon.a1 - lyon.a0 - (3 / 4) * TAU) < 1e-9);
	assert.ok(Math.abs(annecy.a1 - TAU) < 1e-9);
	assert.deepEqual(
		layout.labels.map((l) => l.monitor.id).sort(),
		[3, 4],
		'down and degraded monitors are named at the edge'
	);
	for (const label of layout.labels) assert.ok(Math.abs(label.y2) <= SUN.maxY);
});

test('a single site makes a whole ring and nothing crashes on no data', () => {
	const one = sunburstLayout([{ key: 'local', name: 'Home', monitors: [mon(1)] }]);
	assert.equal(one.sites.length, 1);
	assert.ok(one.sites[0].d.length > 0);
	assert.ok(one.monitors[0].d.length > 0);
	const none = sunburstLayout([]);
	assert.equal(none.total, 0);
	assert.deepEqual(none.labels, []);
});

test('edge labels are capped and spread apart', () => {
	const monitors = Array.from({ length: 30 }, (_, i) => mon(i, i < 20 ? 'down' : 'up'));
	const layout = sunburstLayout([{ key: 'a', name: 'A', monitors }]);
	assert.equal(layout.labels.length, SUN_LABELS);
	for (const side of ['start', 'end']) {
		const ys = layout.labels.filter((l) => l.anchor === side).map((l) => l.y2).sort((a, b) => a - b);
		for (let i = 1; i < ys.length; i++) assert.ok(ys[i] - ys[i - 1] >= 11.999);
	}
	const items = [{ y: 140 }, { y: 141 }, { y: 142 }];
	spread(items, 12, 144);
	assert.deepEqual(
		items.map((i) => i.y),
		[120, 132, 144]
	);
});

test('clip cuts long names with an ellipsis', () => {
	assert.equal(clip('pve-02', 9), 'pve-02');
	assert.equal(clip('api.example.com', 9), 'api.exam…');
});

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
	assert.deepEqual(usedCategories([{ monitors: [mon(1, 'up', 'storage')] }, { monitors: [mon(2, 'up', 'servers'), mon(3, 'up', 'storage')] }]), ['servers', 'storage']);
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

test('load thresholds only apply to percentages; bars stay within 2–100 %', () => {
	assert.equal(loadOf(91, '%'), 'critical');
	assert.equal(loadOf(86, '%'), 'high');
	assert.equal(loadOf(50, '%'), 'normal');
	assert.equal(loadOf(950, 'Mb/s'), 'normal');
	assert.equal(barWidth(40, '%', 0), 40);
	assert.equal(barWidth(200, 'Mb/s', 400), 50);
	assert.equal(barWidth(0, '%', 0), 2);
	assert.equal(barWidth(150, '%', 0), 100);
	assert.equal(barWidth(5, 'Mb/s', 0), 2);
});

test('backup runs are padded at the start and grouped by site', () => {
	const runs = [{ at: 'a', ok: true }, { at: 'b', ok: false }];
	const cells = runCells(runs, 5);
	assert.deepEqual(cells, [null, null, null, true, false]);
	assert.equal(runCells(Array.from({ length: 20 }, () => ({ at: 'x', ok: true })), 14).length, 14);
	const job = (site, status) => ({ job: 'j', engine: 'PBS', detail: '', site, target_id: 1, last_success: null, duration_s: null, changed_bytes: null, runs: [], status });
	const jobs = [job('B', 'ok'), job('A', 'failed'), job('B', 'stale'), job('A', 'running')];
	assert.deepEqual(
		jobsBySite(jobs).map((g) => [g.site, g.jobs.length]),
		[
			['B', 2],
			['A', 2]
		]
	);
	assert.equal(jobsToCheck(jobs), 2);
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
