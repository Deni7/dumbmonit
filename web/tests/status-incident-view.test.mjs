// Public status page: anchors, resolution time, timezone label, service filter. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FILTER_THRESHOLD, draftIncident, durationSeconds, filterGroups, incidentAnchor, incidentFromHash, insertTemplate } from '../src/lib/components/status/incident-view.ts';
import { badgeUrl } from '../src/lib/components/status/share.ts';
import { utcOffset } from '../src/lib/components/status/timezone.ts';

test('an incident has a stable anchor, read back from the hash', () => {
	assert.equal(incidentAnchor(42), 'incident-42');
	assert.equal(incidentFromHash('#incident-42'), 42);
	assert.equal(incidentFromHash('#services'), null);
	assert.equal(incidentFromHash('#incident-x'), null);
});

test('the resolution time is the gap between start and end', () => {
	assert.equal(durationSeconds('2026-09-15T10:00:00Z', '2026-09-15T10:42:00Z'), 2520);
	assert.equal(durationSeconds('2026-09-15 10:00:00', '2026-09-15T11:00:00Z'), 3600);
	assert.equal(durationSeconds('2026-09-15T10:00:00Z', null), null);
	assert.equal(durationSeconds('2026-09-15T10:00:00Z', '2026-09-15T09:00:00Z'), null);
});

test('the filter keeps matching services and drops empty groups', () => {
	const groups = [
		{ name: 'Web', items: [{ label: 'Site', state: 'up' }, { label: 'API', state: 'down' }] },
		{ name: 'Mail', items: [{ label: 'SMTP', state: 'degraded' }] }
	];
	assert.equal(filterGroups(groups, '', false), groups);
	assert.deepEqual(filterGroups(groups, 'api', false).map((g) => g.items.map((i) => i.label)), [['API']]);
	assert.deepEqual(filterGroups(groups, '', true).map((g) => g.name), ['Web', 'Mail']);
	assert.deepEqual(filterGroups(groups, 'mail', true).map((g) => g.name), ['Mail']);
	assert.deepEqual(filterGroups(groups, 'zzz', false), []);
	assert.ok(FILTER_THRESHOLD >= 8);
});

test('the offset reads as UTC plus or minus hours', () => {
	const date = new Date('2026-09-15T10:00:00Z');
	assert.match(utcOffset(date), /^UTC([+-]\d{1,2}(:\d{2})?)?$/);
	assert.equal(utcOffset(new Date(NaN)), 'UTC');
});

test('a badge address carries its style only when it is not the default', () => {
	const O = 'https://x.example';
	assert.equal(badgeUrl(O, 'lab', 'badge'), `${O}/api/public/status/lab/badge.svg`);
	assert.equal(badgeUrl(O, 'lab', 'badge', '', 30, 'flat-square'), `${O}/api/public/status/lab/badge.svg?style=flat-square`);
	assert.equal(badgeUrl(O, 'lab', 'uptime', 'web', 7, 'for-the-badge'), `${O}/api/public/status/lab/components/web/uptime.svg?days=7&style=for-the-badge`);
});

test('a template fills an empty message and follows a written one', () => {
	assert.equal(insertTemplate('', 'Investigating.'), 'Investigating.');
	assert.equal(insertTemplate('  ', 'Investigating.'), 'Investigating.');
	assert.equal(insertTemplate('Disk full.  ', 'Fixed.'), 'Disk full.\n\nFixed.');
});

test('the preview adds the message being written and closes a resolved incident', () => {
	const base = { kind: 'incident', title: ' DB down ', severity: 'major', status: 'resolved', startsAt: '2026-09-15T10:00:00Z', endsAt: null, updates: [{ status: 'investigating', body: 'Looking.', created_at: '2026-09-15T10:00:00Z' }], now: '2026-09-15T10:42:00Z' };
	const draft = draftIncident({ ...base, message: ' Fixed. ' });
	assert.equal(draft.title, 'DB down');
	assert.equal(draft.updates.length, 2);
	assert.equal(draft.updates[1].body, 'Fixed.');
	assert.equal(draft.ends_at, '2026-09-15T10:42:00Z');
	const quiet = draftIncident({ ...base, status: 'monitoring', message: '' });
	assert.equal(quiet.updates.length, 1);
	assert.equal(quiet.ends_at, null);
});
