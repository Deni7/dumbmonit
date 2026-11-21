// SLO tab: budget tone and bar, burn-rate words, target parsing. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	allowedMinutes,
	budgetFill,
	budgetTone,
	formatAvailability,
	formatRate,
	parseObjective,
	shownRate,
	verdictTone
} from '../src/lib/slo.ts';

const status = (remaining) => ({
	measured: remaining !== null,
	monitors: 1,
	attained: null,
	budget: remaining === null ? null : { total_minutes: 43.2, consumed_minutes: 0, remaining_minutes: 0, remaining_percent: remaining },
	burn: { m5: null, m30: null, h1: null, h6: null },
	verdict: 'none'
});

test('99.9 % over 30 days allows 43.2 minutes of downtime', () => {
	assert.ok(Math.abs(allowedMinutes(99.9, 30) - 43.2) < 0.001);
	assert.ok(Math.abs(allowedMinutes(99, 7) - 100.8) < 0.001);
});

test('the target accepts a comma and stays strictly between 0 and 100', () => {
	assert.equal(parseObjective('99,9'), 99.9);
	assert.equal(parseObjective(' 99.95 '), 99.95);
	assert.equal(parseObjective('100'), null);
	assert.equal(parseObjective('0'), null);
	assert.equal(parseObjective('abc'), null);
	assert.equal(parseObjective(''), null);
});

test('the budget bar never overflows its track', () => {
	assert.equal(budgetFill(62.5), 62.5);
	assert.equal(budgetFill(-40), 0);
	assert.equal(budgetFill(140), 100);
	assert.equal(budgetFill(null), 0);
});

test('the budget tone: spent, low, fine, unmeasured', () => {
	assert.equal(budgetTone(status(-1)), 'warning');
	assert.equal(budgetTone(status(0)), 'warning');
	assert.equal(budgetTone(status(10)), 'advisory');
	assert.equal(budgetTone(status(80)), 'signal');
	assert.equal(budgetTone(status(null)), 'ghost');
});

test('burn rates read like the alerts they raise', () => {
	assert.equal(verdictTone('fast'), 'warning');
	assert.equal(verdictTone('slow'), 'advisory');
	assert.equal(verdictTone('none'), 'signal');
	const burn = { m5: 20, m30: 8, h1: 15.24, h6: 6.5 };
	assert.equal(shownRate(burn, 'fast'), 15.24);
	assert.equal(shownRate(burn, 'slow'), 6.5);
	assert.equal(shownRate({ ...burn, h1: null }, 'none'), 6.5);
	assert.equal(formatRate(15.24), '15×');
	assert.equal(formatRate(1.24), '1.2×');
	assert.equal(formatRate(0), '0×');
	assert.equal(formatRate(null), null);
});

test('an availability carries one decimal more than its target', () => {
	assert.equal(formatAvailability(99.9512, 99.9), '99.95 %');
	assert.equal(formatAvailability(99.95123, 99.95), '99.951 %');
	assert.equal(formatAvailability(98.5, 99), '98.50 %');
});
