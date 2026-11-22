// Helpers behind the monitor page and form: maintenance windows covering a
// monitor, previous / next monitor, tags a copy must not carry.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { coversMonitor, windowsFor } from '../src/lib/components/devices/maintenance.ts';
import { neighbours } from '../src/lib/components/devices/neighbours.ts';
import { withoutSecretTags } from '../src/lib/components/device-form/discovery-prefill.ts';

const target = { id: 7, tags: { env: 'prod', keyword: 'ok' } };
const silence = (id, extra = {}) => ({
	id,
	name: `s${id}`,
	comment: '',
	target_id: null,
	matchers: {},
	schedule: { kind: 'once', starts_at: '2026-01-01T00:00:00Z', ends_at: '2026-01-02T00:00:00Z' },
	enabled: true,
	active_now: false,
	active_until: null,
	next_start_at: '2026-06-01T00:00:00Z',
	...extra
});

test('a window covers its own monitor, every monitor, or the ones matching its tags', () => {
	assert.equal(coversMonitor(silence(1, { target_id: 7 }), target), true);
	assert.equal(coversMonitor(silence(2, { target_id: 8 }), target), false);
	assert.equal(coversMonitor(silence(3), target), true);
	assert.equal(coversMonitor(silence(4, { matchers: { tag_env: 'prod' } }), target), true);
	assert.equal(coversMonitor(silence(5, { matchers: { tag_env: 'dev' } }), target), false);
});

test('a snooze of a single alert is not a maintenance window of the monitor', () => {
	assert.equal(coversMonitor(silence(1, { target_id: 7, matchers: { alertname: 'cpu' } }), target), false);
	assert.equal(coversMonitor(silence(2, { matchers: { rule: 'cpu' } }), target), false);
});

test('active windows come first, finished and disabled ones are dropped', () => {
	const list = [
		silence(1, { next_start_at: '2026-07-01T00:00:00Z' }),
		silence(2, { active_now: true, active_until: '2026-05-01T12:00:00Z', next_start_at: null }),
		silence(3, { next_start_at: '2026-06-01T00:00:00Z' }),
		silence(4, { enabled: false }),
		silence(5, { next_start_at: null }),
		silence(6, { target_id: 99 })
	];
	assert.deepEqual(windowsFor(list, target).map((s) => s.id), [2, 3, 1]);
});

test('previous and next follow the order, without wrapping', () => {
	assert.deepEqual(neighbours([1, 2, 3], 2), { prev: 1, next: 3 });
	assert.deepEqual(neighbours([1, 2, 3], 1), { prev: null, next: 2 });
	assert.deepEqual(neighbours([1, 2, 3], 3), { prev: 2, next: null });
	assert.deepEqual(neighbours([1, 2, 3], 9), { prev: null, next: null });
});

test('a copy drops the tags that hold a secret and keeps the others', () => {
	const kept = withoutSecretTags({ keyword: 'ok', api_key: 'x', headers: 'Authorization: y', port: '8080', room: 'r1', Password: 'z' });
	assert.deepEqual(kept, { keyword: 'ok', port: '8080', room: 'r1' });
});
