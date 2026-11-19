// "Coming up" on the Overview: order, the 30 day horizon, one row per monitor for vulnerabilities. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ROW_LIMIT, toneOf, upcomingRows } from '../src/lib/components/overview/upcoming.ts';

const deadline = (over) => ({
	target: 1,
	target_name: 'edge-01',
	target_kind: 'agent',
	kind: 'certificate',
	what: 'a.example',
	expires_at: '2026-12-01T00:00:00',
	days_left: 10,
	...over
});
const finding = (over) => ({
	target_id: 2,
	target_name: 'nas',
	severity: 'critical',
	state: 'vulnerable',
	ignored: false,
	exploited: false,
	cve_id: 'CVE-2026-0001',
	...over
});
const none = { deadlines: [], findings: [], update: null };

test('nothing due gives no row', () => {
	assert.deepEqual(upcomingRows(none), []);
});

test('late first, then by date; the update has no date and comes last', () => {
	const rows = upcomingRows({
		deadlines: [
			deadline({ what: 'far', days_left: 25 }),
			deadline({ what: 'late', days_left: -2, kind: 'backup', value: 4 }),
			deadline({ what: 'disk', days_left: 6, kind: 'disk', value: 91.2 })
		],
		findings: [finding()],
		update: { update_available: true, latest: '0.20.0', current: '0.19.0' }
	});
	assert.deepEqual(
		rows.map((r) => r.what || r.kind),
		['late', 'vulnerability', 'disk', 'far', 'update']
	);
	assert.equal(rows[0].value, 4);
	assert.equal(rows.at(-1).target, null);
	assert.equal(rows.at(-1).href, '/settings/system#about');
});

test('beyond the horizon is left out', () => {
	const rows = upcomingRows({ ...none, deadlines: [deadline({ days_left: 45 }), deadline({ what: 'b', days_left: 30 })] });
	assert.deepEqual(rows.map((r) => r.what), ['b']);
});

test('one vulnerability row per monitor: only critical, still vulnerable, not ignored', () => {
	const rows = upcomingRows({
		...none,
		findings: [
			finding(),
			finding({ cve_id: 'CVE-2026-0002', exploited: true }),
			finding({ severity: 'high' }),
			finding({ state: 'patched' }),
			finding({ ignored: true }),
			finding({ target_id: 3, target_name: 'web' })
		]
	});
	assert.equal(rows.length, 2);
	const nas = rows.find((r) => r.target === 2);
	assert.equal(nas.count, 2);
	assert.equal(nas.exploited, true);
	assert.equal(nas.href, '/targets/2');
});

test('no update row unless one is available', () => {
	assert.equal(upcomingRows({ ...none, update: { update_available: false, latest: '0.19.0', current: '0.19.0' } }).length, 0);
	assert.equal(upcomingRows({ ...none, update: { update_available: true, latest: null, current: '0.19.0' } }).length, 0);
});

test('tone: late or within a day is urgent, within a week soon, the rest calm', () => {
	assert.equal(toneOf(-3), 'urgent');
	assert.equal(toneOf(0.5), 'urgent');
	assert.equal(toneOf(6), 'soon');
	assert.equal(toneOf(12), 'calm');
	assert.equal(toneOf(null), 'calm');
	assert.ok(ROW_LIMIT >= 5);
});
