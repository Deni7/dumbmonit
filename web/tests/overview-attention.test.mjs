// The Overview's "Needs attention" and incidents of the week. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { attentionRows, attentionMonitors, incidentWeek } from '../src/lib/components/overview/attention.ts';

const target = (id) => ({ id, name: `m${id}` });
const device = (id) => ({ kind: 'device', key: `device:${id}`, rank: 0, target: target(id) });
const alert = (fingerprint, over = {}, row = {}) => ({
	kind: 'alert',
	key: `alert:${fingerprint}`,
	rank: 1,
	alert: { fingerprint, effective_phase: 'firing', acked: false, silenced: false, severity: 'warning', ...over },
	...row
});

test('only what is broken now: firing, not acked, not snoozed, not info, not pending', () => {
	const rows = [
		device(1),
		alert('a', {}, { target: target(2) }),
		alert('b', { acked: true }),
		alert('c', { silenced: true }),
		alert('d', { severity: 'info' }),
		alert('e', { effective_phase: 'pending' }),
		alert('f', { effective_phase: 'suppressed' }, { parent: target(1) })
	];
	assert.deepEqual(
		attentionRows(rows).map((r) => r.row.key),
		['device:1', 'alert:a']
	);
});

test('guests held back are counted once, on the parent row', () => {
	const rows = [
		device(1),
		alert('x', { effective_phase: 'suppressed' }, { parent: target(1) }),
		alert('y', { effective_phase: 'suppressed' }, { parent: target(1) }),
		alert('z', { effective_phase: 'suppressed' }, { parent: target(9) })
	];
	const out = attentionRows(rows);
	assert.equal(out.length, 1);
	assert.equal(out[0].held, 2);
});

test('a parent voiced by its own alert carries the guests on that alert row', () => {
	const rows = [
		alert('host-down', {}, { target: target(3) }),
		alert('cpu', {}, { target: target(3) }),
		alert('g', { effective_phase: 'suppressed' }, { parent: target(3) })
	];
	assert.deepEqual(
		attentionRows(rows).map((r) => r.held),
		[1, 0]
	);
});

test('monitors are counted once, alerts without a monitor apart', () => {
	const rows = attentionRows([device(1), alert('a', {}, { target: target(1) }), alert('b'), alert('c', {}, { target: target(2) })]);
	assert.equal(attentionMonitors(rows), 3);
});

test('incidents of the week: flapping counts each firing, durations clipped per day, week before compared', () => {
	const now = new Date(2026, 10, 10, 15, 0, 0).getTime();
	const utc = (d) => d.toISOString().replace('Z', '');
	const row = (fingerprint, target_id, when, from_phase, to_phase) => ({
		at: utc(when),
		fingerprint,
		target_id,
		from_phase,
		to_phase
	});
	const history = [
		// m1 flaps twice today: 1 h then still open since 14:00 (1 h until now).
		row('a', 1, new Date(2026, 10, 10, 9), 'pending', 'firing'),
		row('a', 1, new Date(2026, 10, 10, 10), 'firing', 'resolved'),
		row('a', 1, new Date(2026, 10, 10, 14), 'pending', 'firing'),
		// m2 across midnight: yesterday 23:00 to today 01:00.
		row('b', 2, new Date(2026, 10, 9, 23), 'pending', 'firing'),
		row('b', 2, new Date(2026, 10, 10, 1), 'firing', 'resolved'),
		// previous week.
		row('c', 3, new Date(2026, 10, 2, 8), 'pending', 'firing'),
		row('c', 3, new Date(2026, 10, 2, 9), 'firing', 'resolved'),
		// a pending that never fired is no incident.
		row('d', 4, new Date(2026, 10, 10, 12), 'inactive', 'pending')
	];
	const week = incidentWeek(history, now);
	assert.equal(week.days.length, 7);
	assert.equal(week.days[6].day, Math.round(new Date(2026, 10, 10).getTime() / 1000));
	assert.equal(week.days[6].count, 2);
	assert.equal(week.days[5].count, 1);
	assert.equal(week.count, 3);
	assert.equal(week.days[5].seconds, 3600);
	assert.equal(week.days[6].seconds, 3600 + 3600 + 3600);
	assert.equal(week.seconds, 3600 * 4);
	assert.equal(week.previousCount, 1);
	assert.equal(week.previousSeconds, 3600);
	assert.deepEqual(week.noisiest, { target: 1, count: 2 });
});

test('incidents of the week: nothing in the history is an empty week', () => {
	const week = incidentWeek([], Date.now());
	assert.equal(week.count, 0);
	assert.equal(week.noisiest, null);
});
