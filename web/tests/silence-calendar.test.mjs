// Maintenance calendar layout math: view days, occurrence splitting, query window. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	addDays,
	chipKind,
	clockLabel,
	moveFocus,
	prefillStart,
	queryWindow,
	segmentsByDay,
	shiftAnchor,
	viewDays
} from '../src/lib/components/alerts/calendar-layout.ts';

const occ = (over) => ({
	silence_id: 1,
	name: 'Backups',
	starts_at: new Date(2026, 10, 8, 2, 0).toISOString(),
	ends_at: new Date(2026, 10, 8, 4, 0).toISOString(),
	state: 'upcoming',
	recurring: false,
	enabled: true,
	...over
});

test('month view is whole weeks starting on the chosen day', () => {
	const monday = viewDays('month', '2026-11-15', 1);
	assert.equal(monday.length % 7, 0);
	assert.equal(monday[0], '2026-10-26');
	assert.equal(monday[monday.length - 1], '2026-12-06');
	const sunday = viewDays('month', '2026-11-15', 0);
	assert.equal(sunday[0], '2026-11-01');
});

test('week view is seven days around the anchor', () => {
	const days = viewDays('week', '2026-11-12', 1);
	assert.deepEqual([days[0], days[6]], ['2026-11-09', '2026-11-15']);
});

test('shifting moves by a month or a week', () => {
	assert.equal(shiftAnchor('month', '2026-01-31', 1), '2026-02-28');
	assert.equal(shiftAnchor('week', '2026-11-12', -1), '2026-11-05');
});

test('an occurrence crossing midnight is split over both days', () => {
	const night = occ({
		starts_at: new Date(2026, 10, 7, 23, 0).toISOString(),
		ends_at: new Date(2026, 10, 8, 1, 0).toISOString()
	});
	const map = segmentsByDay([night], ['2026-11-07', '2026-11-08', '2026-11-09']);
	const first = map.get('2026-11-07')[0];
	const second = map.get('2026-11-08')[0];
	assert.equal(first.startMinute, 23 * 60);
	assert.equal(first.endMinute, 1440);
	assert.equal(first.continuesAfter, true);
	assert.equal(second.startMinute, 0);
	assert.equal(second.endMinute, 60);
	assert.equal(second.continuesBefore, true);
	assert.equal(map.get('2026-11-09').length, 0);
});

test('segments of a day are sorted by start', () => {
	const late = occ({ silence_id: 2, starts_at: new Date(2026, 10, 8, 20, 0).toISOString(), ends_at: new Date(2026, 10, 8, 21, 0).toISOString() });
	const map = segmentsByDay([late, occ()], ['2026-11-08']);
	assert.deepEqual(map.get('2026-11-08').map((s) => s.occurrence.silence_id), [1, 2]);
});

test('query window spans the first midnight to the one after the last day', () => {
	const { from, to } = queryWindow(['2026-11-08', '2026-11-09']);
	assert.equal(from, new Date(2026, 10, 8).toISOString());
	assert.equal(to, new Date(2026, 10, 10).toISOString());
});

test('chip kind names the state, recurring only when upcoming', () => {
	assert.equal(chipKind(occ({ state: 'active', recurring: true })), 'active');
	assert.equal(chipKind(occ({ recurring: true })), 'recurring');
	assert.equal(chipKind(occ()), 'upcoming');
	assert.equal(chipKind(occ({ state: 'past' })), 'past');
});

test('clock labels, prefill and keyboard moves', () => {
	assert.equal(clockLabel(125), '02:05');
	assert.equal(clockLabel(1440), '23:59');
	assert.equal(prefillStart('2026-12-01', '2026-11-22'), '2026-12-01T02:00');
	assert.equal(moveFocus('ArrowDown', '2026-11-22'), addDays('2026-11-22', 7));
	assert.equal(moveFocus('a', '2026-11-22'), null);
});
