// Tab title helpers. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pageTitle, withCount, dottedFaviconHref } from '../src/lib/tab-title.ts';

test('page titles use the instance name, or the brand', () => {
	assert.equal(pageTitle('Alerts', 'Homelab'), 'Alerts · Homelab');
	assert.equal(pageTitle('Alerts', '  '), 'Alerts · DumbMonit');
	assert.equal(pageTitle('Alerts'), 'Alerts · DumbMonit');
	assert.equal(pageTitle('', 'Homelab'), 'Homelab');
});

test('the critical count is prefixed once and removed at zero', () => {
	assert.equal(withCount('DumbMonit', 2), '(2) DumbMonit');
	assert.equal(withCount('(2) DumbMonit', 3), '(3) DumbMonit');
	assert.equal(withCount('(3) DumbMonit', 0), 'DumbMonit');
	assert.equal(withCount('X', 150), '(99+) X');
	assert.equal(withCount('(99+) X', 0), 'X');
});

test('the dotted favicon is an SVG data URL', () => {
	assert.match(dottedFaviconHref(), /^data:image\/svg\+xml,/);
	assert.ok(decodeURIComponent(dottedFaviconHref()).includes('#e5484d'));
});
