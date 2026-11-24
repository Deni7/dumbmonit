// Overview helpers: durations, availability, the monitors strip, problem rows, recently
// resolved, sites, disk forecasts and unique keys for keyed lists. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { availabilityText, durationParts, monitorBars } from '../src/lib/components/overview/dash/parts/format.ts';
import { actorName, initials, problemRows, recentResolved } from '../src/lib/components/overview/dash/parts/problems.ts';
import { groupBySite, siteIndex } from '../src/lib/components/overview/dash/parts/site-index.ts';
import { withDiskForecasts } from '../src/lib/components/overview/dash/parts/expiries.ts';
import { uniqueKeys } from '../src/lib/components/overview/dash/parts/keys.ts';

const close = (actual, expected, eps = 1e-6) => assert.ok(Math.abs(actual - expected) < eps, `${actual} ≈ ${expected}`);

// --- format -------------------------------------------------------------------

test('durations keep their two most significant parts', () => {
	assert.deepEqual(durationParts(40), [{ n: 40, unit: 's' }]);
	assert.deepEqual(durationParts(14 * 60), [{ n: 14, unit: 'min' }]);
	assert.deepEqual(durationParts(3 * 3600 + 2 * 60 + 5), [{ n: 3, unit: 'h' }, { n: 2, unit: 'min' }]);
	assert.deepEqual(durationParts(3 * 3600), [{ n: 3, unit: 'h' }]);
	assert.deepEqual(durationParts(2 * 86400 + 4 * 3600), [{ n: 2, unit: 'd' }, { n: 4, unit: 'h' }]);
	assert.deepEqual(durationParts(-5), [{ n: 0, unit: 's' }]);
});

test('availability is written with two decimals, 100 only when it is', () => {
	assert.equal(availabilityText(99.9996), '100');
	assert.equal(availabilityText(99.95), '99.95');
	assert.equal(availabilityText(null), '—');
});

test('the monitors strip groups bars by site, a down monitor stands tall', () => {
	const out = monitorBars([
		{ key: 'a', name: 'A', statuses: ['up', 'down'] },
		{ key: 'empty', name: 'Empty', statuses: [] },
		{ key: 'b', name: 'B', statuses: ['paused'] }
	]);
	assert.equal(out.bars.length, 3);
	close(out.bars[0].h, 26 * 0.62);
	assert.equal(out.bars[1].h, 26);
	assert.equal(out.bars[1].y, 0);
	close(out.bars[2].x, 16.8);
	close(out.bars[2].h, 26 * 0.3);
	assert.deepEqual(
		out.labels.map((l) => l.key),
		['a', 'b']
	);
	close(out.labels[0].share, 7.8);
	close(out.labels[1].share, 3.2);
	close(out.width, 20);
});

// --- problems -----------------------------------------------------------------

const device = (id, over = {}) => ({
	kind: 'device',
	key: `device:${id}`,
	rank: 0,
	tone: 'warning',
	plate: 'Down',
	target: { id, name: `m${id}` },
	state: 'down',
	detail: '',
	since: null,
	...over
});
const alertRow = (fingerprint, alert, over = {}) => ({
	kind: 'alert',
	key: `alert:${fingerprint}`,
	rank: 0,
	tone: 'warning',
	plate: 'Warning',
	alert: {
		fingerprint,
		effective_phase: 'firing',
		severity: 'warning',
		acked: false,
		acked_by: null,
		silenced: false,
		...alert
	},
	since: null,
	...over
});

test('problem rows: open first, taken ones after, info and building up left out', () => {
	const rows = [
		device(1),
		alertRow('a1', { severity: 'critical' }, { target: { id: 2 } }),
		alertRow('info', { severity: 'info' }),
		alertRow('pending', { effective_phase: 'pending' }),
		alertRow('held', { effective_phase: 'suppressed' }, { parent: { id: 1 } }),
		alertRow('muted', { silenced: true }, { target: { id: 4 } }),
		alertRow('acked', { acked: true, acked_by: 'Sam' }, { target: { id: 3 } })
	];
	const out = problemRows(rows);
	assert.deepEqual(
		out.map((p) => p.row.key),
		['device:1', 'alert:a1', 'alert:muted', 'alert:acked']
	);
	assert.equal(out[0].held, 1);
	assert.equal(out[0].acked, null);
	assert.equal(out[2].muted, true);
	assert.deepEqual(out[3].acked, { by: 'Sam' });
});

test('a monitor acknowledged until it answers moves below the open rows', () => {
	const out = problemRows([device(1), device(2)], new Set([1]));
	assert.deepEqual(
		out.map((p) => p.row.key),
		['device:2', 'device:1']
	);
	assert.deepEqual(out[1].acked, { by: null });
});

test('who acknowledged, and their initials', () => {
	assert.equal(actorName('token:ci'), 'ci');
	assert.equal(actorName('sam'), 'sam');
	assert.equal(actorName(null), null);
	assert.equal(initials('Sam'), 'S');
	assert.equal(initials('alex.martin'), 'AM');
	assert.equal(initials(''), '?');
});

const entry = (fingerprint, at, from_phase, to_phase, over = {}) => ({
	at,
	fingerprint,
	rule_uid: `r-${fingerprint}`,
	target_id: 1,
	from_phase,
	to_phase,
	severity: 'warning',
	notified: true,
	...over
});

test('recently resolved: newest first, with how long it lasted', () => {
	const out = recentResolved([
		entry('f4', '2026-11-20T12:00:00Z', 'firing', 'resolved'),
		entry('f1', '2026-11-20T10:03:00Z', 'firing', 'resolved'),
		entry('f1', '2026-11-20T10:00:00Z', 'pending', 'firing'),
		entry('f2', '2026-11-20T09:00:00Z', 'pending', 'resolved'),
		entry('f3', '2026-11-20T11:00:00Z', 'pending', 'firing', { severity: 'info' }),
		entry('f3', '2026-11-20T11:05:00Z', 'firing', 'resolved', { severity: 'info' })
	]);
	assert.deepEqual(
		out.map((r) => [r.fingerprint, r.lasted]),
		[
			['f4', null],
			['f1', 180]
		]
	);
	assert.equal(recentResolved([], 3).length, 0);
});

// --- sites ----------------------------------------------------------------------

const sites = [
	{ key: 'local', name: 'HQ', monitors: [{ id: 1, kind: 'Linux agent' }, { id: 2 }] },
	{ key: 'relay:colo', name: 'Colo', monitors: [{ id: 3, kind: 'HTTPS check' }] },
	{ key: 'relay:empty', name: 'Empty', monitors: [] }
];

test('monitors gathered by site; unknown ones go to the first site', () => {
	assert.equal(siteIndex(sites).get(3).siteName, 'Colo');
	assert.equal(siteIndex(sites).get(1).kind, 'Linux agent');
	const groups = groupBySite([{ id: 3 }, { id: 1 }, { id: 9 }], sites, 'This site');
	assert.deepEqual(
		groups.map((g) => [g.key, g.monitors.map((m) => m.id)]),
		[
			['local', [1, 9]],
			['relay:colo', [3]]
		]
	);
	assert.deepEqual(groupBySite([{ id: 5 }], [], 'This site'), [{ key: 'local', name: 'This site', monitors: [{ id: 5 }] }]);
	assert.deepEqual(groupBySite([], [], 'This site'), []);
});

// --- expiries -------------------------------------------------------------------------

test('disk forecasts join the expiries unless a deadline already names the disk', () => {
	const row = (key, kind, target, days) => ({ key, kind, what: '', monitor: null, target, days, at: '', href: '', tone: 'calm' });
	const out = withDiskForecasts(
		[row('update', 'update', null, null), row('certificate:2:x', 'certificate', 2, 20), row('disk:1:/', 'disk', 1, 5)],
		[
			{ target_id: 1, label: 'nas /', site: 'HQ', value: 90, unit: '%', series: [], forecast_days: 3 },
			{ target_id: 4, label: 'syn-01 Volume 2', site: 'HQ', value: 86, unit: '%', series: [], forecast_days: 11 },
			{ target_id: 5, label: 'far', site: '', value: 10, unit: '%', series: [], forecast_days: 200 },
			{ target_id: 6, label: 'unknown', site: '', value: 10, unit: '%', series: [], forecast_days: null }
		]
	);
	assert.deepEqual(
		out.map((r) => r.key),
		['disk:1:/', 'forecast:4:syn-01 Volume 2', 'certificate:2:x', 'update']
	);
	const forecast = out[1];
	assert.equal(forecast.monitor, 'HQ');
	assert.equal(forecast.value, 86);
	assert.equal(forecast.tone, 'calm');
	assert.equal(forecast.href, '/targets/4');
});

// --- keys of keyed lists ---------------------------------------------------------------

test('unique keys: the first keeps its key, repeats get a suffix, nothing collides', () => {
	assert.deepEqual(uniqueKeys([], (x) => x), []);
	assert.deepEqual(
		uniqueKeys(['a', 'b', 'c'], (x) => x),
		['a', 'b', 'c']
	);
	// A certificate probed from two places: two deadlines with the same natural key.
	assert.deepEqual(
		uniqueKeys([{ key: 'certificate:12:nas' }, { key: 'certificate:12:nas' }, { key: 'disk:3:/' }], (r) => r.key),
		['certificate:12:nas', 'certificate:12:nas#2', 'disk:3:/']
	);
	// A generated key never takes one the list already uses.
	assert.deepEqual(
		uniqueKeys(['a', 'a#2', 'a', 'a'], (x) => x),
		['a', 'a#2', 'a#3', 'a#4']
	);
	assert.deepEqual(
		uniqueKeys(['a', 'a', 'a#2'], (x) => x),
		['a', 'a#3', 'a#2']
	);
	// Numbers (monitor ids) are keys too.
	assert.deepEqual(
		uniqueKeys([7, 7, 8], (x) => x),
		['7', '7#2', '8']
	);
	for (const list of [['x', 'x', 'x#2', 'x#3', 'x'], ['', '', '#2']]) {
		const keys = uniqueKeys(list, (x) => x);
		assert.equal(new Set(keys).size, list.length);
	}
});
