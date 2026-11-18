// On-call schedules are optional: hidden unless turned on. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { withoutOnCall } from '../src/lib/components/oncall/gate.ts';

const kinds = [{ kind: 'ntfy' }, { kind: 'oncall' }, { kind: 'grafana_oncall' }];

test('the on-call channel kind is hidden while the feature is off', () => {
	assert.deepEqual(
		withoutOnCall(kinds, false).map((k) => k.kind),
		['ntfy', 'grafana_oncall']
	);
});

test('everything is kept when the feature is on', () => {
	assert.equal(withoutOnCall(kinds, true).length, 3);
});
