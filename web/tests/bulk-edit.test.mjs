// The bulk edit dialog's request and report (`monitors/bulk-edit.ts`).
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	KEEP,
	NEW_FOLDER,
	NO_FOLDER,
	buildBulkPatch,
	bulkEditFailures,
	freeTagKeys,
	parseTag
} from '../src/lib/components/monitors/bulk-edit.ts';

const form = (extra = {}) => ({
	interval: KEEP,
	channelMode: 'keep',
	channels: new Set(),
	tagToAdd: '',
	tagsToRemove: new Set(),
	folder: KEEP,
	newFolder: '',
	...extra
});

test('an untouched form changes nothing', () => {
	assert.equal(buildBulkPatch(form()), null);
	assert.equal(buildBulkPatch(form({ channelMode: 'add' })), null, 'adding no channel is no change');
	assert.equal(buildBulkPatch(form({ folder: NEW_FOLDER, newFolder: '  ' })), null);
});

test('each field becomes its part of the request', () => {
	const patch = buildBulkPatch(
		form({
			interval: '300',
			channelMode: 'remove',
			channels: new Set([5, 2]),
			tagToAdd: ' room = rack-1 ',
			tagsToRemove: new Set(['env', 'room']),
			folder: 'Edge'
		})
	);
	assert.deepEqual(patch, {
		interval_secs: 300,
		channels: { remove: [2, 5] },
		tags: { add: { room: 'rack-1' }, remove: ['env'] },
		group_name: 'Edge'
	});
});

test('replacing with no channel goes back to the rules, and "No folder" empties the folder', () => {
	assert.deepEqual(buildBulkPatch(form({ channelMode: 'set', folder: NO_FOLDER })), { channels: { set: [] }, group_name: '' });
	assert.deepEqual(buildBulkPatch(form({ folder: NEW_FOLDER, newFolder: ' Lab ' })), { group_name: 'Lab' });
});

test('a tag is key=value or a bare key', () => {
	assert.deepEqual(parseTag('owner'), { key: 'owner', value: '' });
	assert.deepEqual(parseTag('a=b=c'), { key: 'a', value: 'b=c' });
	assert.equal(parseTag('=x'), null);
	assert.equal(parseTag('  '), null);
});

test('free tag keys of a selection leave the options of each type out', () => {
	const targets = [
		{ id: 1, name: 'a', kind: 'http', tags: { port: '443', env: 'prod' } },
		{ id: 2, name: 'b', kind: 'snmp', tags: { room: 'r1', env: 'lab' } }
	];
	const keys = freeTagKeys(targets, (kind) => new Set(kind === 'http' ? ['port'] : []));
	assert.deepEqual(keys, ['env', 'room']);
});

test('the monitors left as they were are named with the reason', () => {
	const report = {
		updated: 1,
		failed: 2,
		results: [
			{ id: 1, ok: true },
			{ id: 2, ok: false, error: 'Managed by labels.' },
			{ id: 9, ok: false, error: 'Monitor 9 not found.' }
		]
	};
	const { ids, failed } = bulkEditFailures(report, [{ id: 1, name: 'one' }, { id: 2, name: 'two' }]);
	assert.deepEqual(ids, [2, 9]);
	assert.deepEqual(failed, [
		{ name: 'two', error: 'Managed by labels.' },
		{ name: '#9', error: 'Monitor 9 not found.' }
	]);
});
