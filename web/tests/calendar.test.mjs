// Date pickers: calendar arithmetic on YYYY-MM-DD, week start by locale, range shortcuts,
// typed times. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	addDays,
	addMonths,
	clampIso,
	firstDayOfRegion,
	firstDayOfWeek,
	joinDateTime,
	matchingPreset,
	monthGrid,
	parseIsoDate,
	parseTime,
	presetRange,
	shiftTime,
	splitDateTime,
	startOfWeek,
	weekdayOrder
} from '../src/lib/ui/calendar.ts';

test('only real days parse', () => {
	assert.deepEqual(parseIsoDate('2026-02-28'), { year: 2026, month: 2, day: 28 });
	assert.equal(parseIsoDate('2026-02-30'), null);
	assert.equal(parseIsoDate('2026-2-3'), null);
	assert.equal(parseIsoDate(''), null);
	assert.equal(parseIsoDate(undefined), null);
});

test('days and months move without drifting', () => {
	assert.equal(addDays('2026-03-31', 1), '2026-04-01');
	assert.equal(addDays('2026-01-01', -1), '2025-12-31');
	// The day of the month is kept inside the shorter month.
	assert.equal(addMonths('2026-01-31', 1), '2026-02-28');
	assert.equal(addMonths('2024-01-31', 1), '2024-02-29');
	assert.equal(addMonths('2026-03-15', -3), '2025-12-15');
	assert.equal(clampIso('2026-05-01', '2026-05-10', '2026-05-20'), '2026-05-10');
	assert.equal(clampIso('2026-05-30', '', '2026-05-20'), '2026-05-20');
});

test('the month grid is six weeks from the first day of the week', () => {
	// November 2026 starts on a Sunday.
	const monday = monthGrid('2026-11-18', 1);
	assert.equal(monday.length, 6);
	assert.ok(monday.every((week) => week.length === 7));
	assert.equal(monday[0][0], '2026-10-26');
	assert.equal(monday[0][6], '2026-11-01');
	const sunday = monthGrid('2026-11-18', 0);
	assert.equal(sunday[0][0], '2026-11-01');
	assert.equal(startOfWeek('2026-11-18', 1), '2026-11-16');
	assert.deepEqual(weekdayOrder(1), [1, 2, 3, 4, 5, 6, 0]);
});

test('the week starts where the locale says', () => {
	assert.equal(firstDayOfWeek('fr'), 1);
	assert.equal(firstDayOfWeek('de'), 1);
	assert.equal(firstDayOfWeek('ru'), 1);
	assert.equal(firstDayOfWeek('en-US'), 0);
	assert.equal(firstDayOfWeek('pt-BR'), 0);
	assert.equal(firstDayOfRegion('US'), 0);
	assert.equal(firstDayOfRegion('FR'), 1);
	assert.equal(firstDayOfRegion('EG'), 6);
	assert.equal(firstDayOfRegion(undefined), 1);
});

test('range shortcuts include both ends', () => {
	const today = '2026-11-18';
	assert.deepEqual(presetRange('last_7_days', today), { from: '2026-11-12', to: today });
	assert.deepEqual(presetRange('last_30_days', today), { from: '2026-10-20', to: today });
	assert.deepEqual(presetRange('this_month', today), { from: '2026-11-01', to: today });
	assert.deepEqual(presetRange('last_month', today), { from: '2026-10-01', to: '2026-10-31' });
	assert.deepEqual(presetRange('last_quarter', today), { from: '2026-07-01', to: '2026-09-30' });
	assert.deepEqual(presetRange('last_quarter', '2026-02-10'), { from: '2025-10-01', to: '2025-12-31' });
	assert.equal(matchingPreset('2026-10-01', '2026-10-31', today), 'last_month');
	assert.equal(matchingPreset('2026-10-02', '2026-10-31', today), null);
});

test('typed times are read as HH:mm', () => {
	assert.equal(parseTime('9'), '09:00');
	assert.equal(parseTime('930'), '09:30');
	assert.equal(parseTime('0930'), '09:30');
	assert.equal(parseTime('9h30'), '09:30');
	assert.equal(parseTime('14:30:15'), '14:30');
	assert.equal(parseTime('2pm'), '14:00');
	assert.equal(parseTime('12:15 am'), '00:15');
	assert.equal(parseTime('24:00'), null);
	assert.equal(parseTime('9:75'), null);
	assert.equal(parseTime('soon'), null);
	assert.equal(shiftTime('23:50', 15), '00:05');
	assert.equal(shiftTime('00:05', -15), '23:50');
});

test('a datetime-local value splits and joins back', () => {
	assert.deepEqual(splitDateTime('2026-09-14T22:00'), { date: '2026-09-14', time: '22:00' });
	assert.deepEqual(splitDateTime(''), { date: '', time: '' });
	assert.equal(joinDateTime('2026-09-14', '7:5'), '2026-09-14T00:00', 'an unreadable time falls back');
	assert.equal(joinDateTime('2026-09-14', '07:05'), '2026-09-14T07:05');
	assert.equal(joinDateTime('', '07:05'), '');
});
