// Overview dashboards, widget helpers: durations, deltas, budgets, bar geometry, versions,
// problem rows, recently resolved, sites, the maintenance strip and disk forecasts. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	availabilityText,
	barWidth,
	budgetTone,
	compareVersions,
	daysLeftFill,
	deltaOf,
	durationParts,
	monitorBars,
	pointDelta,
	signed,
	stackBars,
	versionShares
} from '../src/lib/components/overview/dash/parts/format.ts';
import { actorName, initials, problemRows, recentResolved } from '../src/lib/components/overview/dash/parts/problems.ts';
import { groupBySite, siteIndex } from '../src/lib/components/overview/dash/parts/site-index.ts';
import { maintenanceStrip, nextOccurrences } from '../src/lib/components/overview/dash/parts/timeline.ts';
import { withDiskForecasts } from '../src/lib/components/overview/dash/parts/expiries.ts';
import { eventGroup } from '../src/lib/components/overview/dash/parts/plates.ts';

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

test('a delta says which way and whether it is good news', () => {
	const fewer = deltaOf(30, 61, { lowerIsBetter: true });
	assert.equal(fewer.diff, -31);
	close(fewer.pct, (-31 / 61) * 100);
	assert.equal(fewer.direction, 'down');
	assert.equal(fewer.better, true);

	const fromZero = deltaOf(5, 0);
	assert.equal(fromZero.pct, null);
	assert.equal(fromZero.direction, 'up');
	assert.equal(fromZero.better, true);

	const noise = deltaOf(100, 90, { lowerIsBetter: true, flatWithin: 30 });
	assert.equal(noise.direction, 'flat');
	assert.equal(noise.better, null);

	assert.equal(deltaOf(null, 3), null);
	assert.equal(deltaOf(3, undefined), null);
	assert.equal(deltaOf(Number.NaN, 3), null);
});

test('availability points and their sign', () => {
	assert.equal(pointDelta(99.96, 99.93), 0.03);
	assert.equal(pointDelta(null, 99.9), null);
	assert.equal(signed(0.03), '+0.03');
	assert.equal(signed(-0.12), '−0.12');
	assert.equal(signed(0), '0.00');
	assert.equal(signed(-0.001), '0.00');
	assert.equal(availabilityText(99.9996), '100');
	assert.equal(availabilityText(99.95), '99.95');
	assert.equal(availabilityText(null), '—');
});

test('error budgets and bars', () => {
	assert.equal(budgetTone(null), null);
	assert.equal(budgetTone(0), 'spent');
	assert.equal(budgetTone(-5), 'spent');
	assert.equal(budgetTone(18), 'low');
	assert.equal(budgetTone(62), 'ok');
	assert.equal(barWidth(140), 100);
	assert.equal(barWidth(-3), 0);
	assert.equal(barWidth(null), 0);
	assert.equal(daysLeftFill(null), 0);
	assert.equal(daysLeftFill(0), 100);
	assert.equal(daysLeftFill(90), 4);
	assert.equal(daysLeftFill(45), 50);
	assert.equal(daysLeftFill(9), 90);
});

test('stacked bars put critical at the bottom and mark empty days', () => {
	const out = stackBars(
		[
			{ critical: 1, warning: 2, info: 0 },
			{ critical: 0, warning: 0, info: 0 }
		],
		{ width: 20, height: 40, gap: 2, floor: 4 }
	);
	assert.equal(out.peak, 4);
	assert.deepEqual(out.rects, [
		{ x: 1, y: 30, w: 8, h: 10, severity: 'critical' },
		{ x: 1, y: 10, w: 8, h: 20, severity: 'warning' }
	]);
	assert.deepEqual(out.empty, [{ x: 11, w: 8 }]);
	assert.deepEqual(stackBars([], { width: 10, height: 10 }), { rects: [], empty: [], peak: 0 });
});

test('agent versions: newest first, ranked against the latest', () => {
	assert.ok(compareVersions('v0.20.1', '0.20.0') < 0);
	assert.equal(compareVersions('0.20.0', '0.20.0'), 0);
	const shares = versionShares([
		{ version: '0.19.2', count: 2, latest: false },
		{ version: '0.20.1', count: 14, latest: true },
		{ version: '0.20.0', count: 3, latest: false },
		{ version: '0.18.0', count: 0, latest: false }
	]);
	assert.deepEqual(shares, [
		{ version: '0.20.1', count: 14, rank: 'latest' },
		{ version: '0.20.0', count: 3, rank: 'behind' },
		{ version: '0.19.2', count: 2, rank: 'old' }
	]);
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

// --- maintenance strip ----------------------------------------------------------------

test('maintenance windows are placed on their site, every site for a global one', () => {
	const from = Date.UTC(2026, 10, 23);
	const h = 3600_000;
	const iso = (ms) => new Date(ms).toISOString();
	const occ = (silence_id, start, end, over = {}) => ({
		silence_id,
		name: `w${silence_id}`,
		starts_at: iso(start),
		ends_at: iso(end),
		state: 'upcoming',
		recurring: false,
		enabled: true,
		...over
	});
	const occurrences = [
		occ(1, from + 24 * h, from + 26 * h),
		occ(2, from - 2 * h, from + h, { state: 'active' }),
		occ(1, from + 8 * 24 * h, from + 8 * 24 * h + h),
		occ(1, from + 30 * h, from + 31 * h, { enabled: false })
	];
	const silences = [
		{ id: 1, target_id: 3 },
		{ id: 2, target_id: null }
	];
	const twoSites = [
		{ key: 'a', name: 'A', monitors: [{ id: 1 }] },
		{ key: 'b', name: 'B', monitors: [{ id: 3 }] }
	];
	const now = from + 12 * h;
	const strip = maintenanceStrip({ occurrences, silences, sites: twoSites, from, now });
	close(strip.nowPct, (12 / 168) * 100);
	assert.equal(strip.rows.length, 2);
	assert.deepEqual(
		strip.rows[0].windows.map((w) => w.name),
		['w2']
	);
	assert.deepEqual(
		strip.rows[1].windows.map((w) => w.name),
		['w2', 'w1']
	);
	const global = strip.rows[0].windows[0];
	assert.equal(global.left, 0);
	assert.equal(global.width, 0.9);
	assert.equal(global.active, true);
	const local = strip.rows[1].windows[1];
	close(local.left, (24 / 168) * 100);
	close(local.width, (2 / 168) * 100);

	const lone = maintenanceStrip({ occurrences, silences, sites: [], from, now, fallback: 'Here' });
	assert.deepEqual(
		lone.rows.map((r) => [r.key, r.name, r.windows.length]),
		[['all', 'Here', 2]]
	);

	assert.deepEqual(
		nextOccurrences(occurrences, now).map((o) => o.starts_at),
		[iso(from + 24 * h), iso(from + 8 * 24 * h)]
	);
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

test('events split into alerts and changes', () => {
	assert.equal(eventGroup('crit'), 'alerts');
	assert.equal(eventGroup('ack'), 'alerts');
	assert.equal(eventGroup('maint'), 'changes');
	assert.equal(eventGroup('info'), 'changes');
});
