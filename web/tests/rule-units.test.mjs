// How a rule's threshold is read and typed, from its unit (rule editor).
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	FLAG_UNIT,
	HOLD_UNITS,
	durationBase,
	durationText,
	isFlagUnit,
	splitDuration,
	thresholdShape,
	toStored
} from '../src/lib/components/alerts/rules/units.ts';

test('the unit alone decides the shape of the threshold', () => {
	assert.deepEqual(thresholdShape('bool'), { kind: 'flag' });
	assert.deepEqual(thresholdShape('s'), { kind: 'duration', base: 1, choices: ['s', 'm', 'h', 'd'] });
	assert.deepEqual(thresholdShape('min'), { kind: 'duration', base: 60, choices: ['m', 'h', 'd'] });
	assert.deepEqual(thresholdShape('h'), { kind: 'duration', base: 3600, choices: ['h', 'd'] });
	assert.deepEqual(thresholdShape('d'), { kind: 'duration', base: 86400, choices: ['h', 'd'] });
	assert.deepEqual(thresholdShape('%'), { kind: 'plain', unit: '%' });
	assert.deepEqual(thresholdShape('ms'), { kind: 'plain', unit: 'ms' });
	assert.deepEqual(thresholdShape(''), { kind: 'plain', unit: '' });
	assert.deepEqual(thresholdShape(undefined), { kind: 'plain', unit: '' });
	assert.equal(FLAG_UNIT, 'bool');
	assert.ok(isFlagUnit('bool'));
	assert.ok(!isFlagUnit('s'));
	assert.equal(durationBase('d'), 86400);
	assert.equal(durationBase('%'), null);
});

test('a stored duration reads in the largest whole unit', () => {
	// The Active Backup device rule ships at three days, stored in seconds.
	assert.deepEqual(splitDuration(3 * 86400, 1), { value: 3, unit: 'd' });
	assert.deepEqual(splitDuration(36 * 3600, 1), { value: 36, unit: 'h' });
	assert.deepEqual(splitDuration(900, 1), { value: 15, unit: 'm' });
	assert.deepEqual(splitDuration(90, 1), { value: 90, unit: 's' });
	assert.deepEqual(splitDuration(0.5, 1), { value: 0.5, unit: 's' });
	// Zero reads in the largest unit offered: "0 days" left on a certificate.
	assert.deepEqual(splitDuration(0, 1), { value: 0, unit: 'd' });
	// A rule kept in days: 14 d stays 14 days, half a day becomes 12 hours.
	assert.deepEqual(splitDuration(14, 86400, ['h', 'd']), { value: 14, unit: 'd' });
	assert.deepEqual(splitDuration(0.5, 86400, ['h', 'd']), { value: 12, unit: 'h' });
	// Below the smallest unit offered: a fraction of it.
	assert.deepEqual(splitDuration(0.01, 86400, ['h', 'd']), { value: 0.24, unit: 'h' });
	// Negative values keep their sign.
	assert.deepEqual(splitDuration(-2 * 86400, 1), { value: -2, unit: 'd' });
});

test('what is typed goes back in the stored unit', () => {
	assert.equal(toStored('2', 'd', 1), 172800);
	assert.equal(toStored(' 36 ', 'h', 1), 129600);
	assert.equal(toStored('1.5', 'd', 1), 129600);
	assert.equal(toStored('12', 'h', 86400), 0.5);
	assert.equal(toStored('3', 'd', 86400), 3);
	// A bound number field hands back a number, or null once emptied.
	assert.equal(toStored(5, 'm', 1), 300);
	assert.equal(toStored(null, 'm', 1), null);
	assert.equal(toStored('', 'd', 1), null);
	assert.equal(toStored('abc', 'd', 1), null);
	// The hold duration is whole seconds.
	assert.equal(toStored('0.001', 'm', 1, true), 0);
	assert.equal(toStored('1.5', 'm', 1, true), 90);
});

test('round trip: opening and saving changes nothing', () => {
	for (const stored of [180, 3600, 2 * 86400, 7 * 24 * 3600, 30 * 86400, 0.5, 45]) {
		const { text, unit } = durationText(stored, 1, HOLD_UNITS);
		assert.equal(toStored(text, unit, 1), stored, `${stored} s`);
	}
	for (const stored of [14, 30, 0, 2]) {
		const { text, unit } = durationText(stored, 86400, ['h', 'd']);
		assert.equal(toStored(text, unit, 86400), stored, `${stored} d`);
	}
});

test('an empty value starts on the unit of the field beside it', () => {
	assert.deepEqual(durationText(null, 1, HOLD_UNITS, 'h'), { text: '', unit: 'h' });
	assert.deepEqual(durationText(null, 86400, ['h', 'd'], 'm'), { text: '', unit: 'd' });
	assert.deepEqual(durationText(172800, 1, HOLD_UNITS), { text: '2', unit: 'd' });
});
