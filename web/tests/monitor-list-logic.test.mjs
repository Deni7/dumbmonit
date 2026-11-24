// Filtering, segments and optimistic overrides of the monitors list (`monitors/list.ts`).
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	countActiveFilters,
	countSegments,
	inSegment,
	kindChoices,
	matchCount,
	narrowTargets,
	restoreOverrides,
	targetToPayload,
	toggled,
	troubleCount,
	visibleIds,
	withOverrides
} from '../src/lib/components/monitors/list.ts';

const t = (id, name, extra = {}) => ({
	id,
	name,
	address: `${name}.lan`,
	kind: 'snmp',
	enabled: true,
	tags: {},
	location_id: null,
	position: 0,
	group_name: '',
	parent_id: null,
	via_agent: null,
	interval_secs: 60,
	...extra
});
const NO_KEYS = new Set();
const noOptions = () => NO_KEYS;
const all = { search: '', kind: '', site: '', tags: [] };

test('the search matches the name, the address, the kind, its label and the free tags', () => {
	const list = [
		t(1, 'core-switch'),
		t(2, 'nas', { kind: 'synology' }),
		t(3, 'printer', { tags: { room: 'office' } }),
		t(4, 'web', { address: 'shop.example.com', kind: 'http' })
	];
	const labels = new Map([['synology', 'Synology DSM']]);
	const names = (f) => narrowTargets(list, { ...all, ...f }, labels, noOptions).map((x) => x.id);
	assert.deepEqual(names({ search: 'CORE' }), [1]);
	assert.deepEqual(names({ search: 'dsm' }), [2]);
	assert.deepEqual(names({ search: 'office' }), [3]);
	assert.deepEqual(names({ search: 'shop.example' }), [4]);
	assert.deepEqual(names({ search: '  ' }), [1, 2, 3, 4]);
	assert.deepEqual(names({ kind: 'http' }), [4]);
});

test('an option of the device type is never searched (it may hold a secret)', () => {
	const list = [t(1, 'nas', { tags: { community: 'hunter2' } })];
	const options = () => new Set(['community']);
	assert.deepEqual(narrowTargets(list, { ...all, search: 'hunter2' }, new Map(), options), []);
});

test('site and tags narrow the list too', () => {
	const list = [t(1, 'a', { location_id: 7, tags: { env: 'prod' } }), t(2, 'b'), t(3, 'c', { tags: { env: 'prod' } })];
	const ids = (f) => narrowTargets(list, { ...all, ...f }, new Map(), noOptions).map((x) => x.id);
	assert.deepEqual(ids({ site: '7' }), [1]);
	assert.deepEqual(ids({ site: 'none' }), [2, 3]);
	assert.deepEqual(ids({ tags: ['env=prod'] }), [1, 3]);
});

test('segments count and keep the same devices', () => {
	const list = [t(1, 'a'), t(2, 'b'), t(3, 'c'), t(4, 'd')];
	const states = { 1: 'online', 2: 'down', 3: 'disabled', 4: 'pending' };
	const stateOf = (x) => states[x.id];
	assert.deepEqual(countSegments(list, stateOf), { all: 4, attention: 1, reporting: 1, disabled: 1 });
	assert.deepEqual([...visibleIds(list, 'attention', stateOf)], [2]);
	assert.deepEqual([...visibleIds(list, 'all', stateOf)], [1, 2, 3, 4]);
	assert.equal(inSegment('offline', 'attention'), true);
	assert.equal(inSegment('pending', 'reporting'), false);
});

test('the kind filter offers the kinds in use, by label, plus the one in the URL', () => {
	const list = [t(1, 'a', { kind: 'snmp' }), t(2, 'b', { kind: 'http' }), t(3, 'c', { kind: 'http' })];
	const labels = new Map([['snmp', 'Switch'], ['http', 'Website']]);
	assert.deepEqual(kindChoices(list, '', labels), [['snmp', 'Switch'], ['http', 'Website']]);
	assert.deepEqual(kindChoices(list, 'ping', labels).map(([k]) => k), ['ping', 'snmp', 'http']);
});

test('the Filter button counts what is away from its default, each tag apart', () => {
	assert.equal(countActiveFilters({ segment: 'all', kind: '', site: '', tags: [], sorted: false }), 0);
	assert.equal(countActiveFilters({ segment: 'attention', kind: 'http', site: '3', tags: ['a=1', 'b=2'], sorted: true }), 6);
});

test('rows shown only as context are neither counted nor reported in trouble', () => {
	const rows = [
		{ state: 'down', context: true },
		{ state: 'down' },
		{ state: 'online' },
		{ state: 'misconfigured' }
	];
	assert.equal(matchCount(rows), 3);
	assert.equal(troubleCount(rows), 2);
});

test('overrides apply on top of the store and roll back to what they were', () => {
	const list = [t(1, 'a', { position: 0, group_name: 'x' }), t(2, 'b', { position: 1 })];
	assert.equal(withOverrides(list, new Map(), new Map()), list, 'nothing pending: the same array');
	const moved = withOverrides(list, new Map([[2, 0]]), new Map([[1, 'y']]));
	assert.deepEqual(
		moved.map((x) => [x.id, x.position, x.group_name]),
		[
			[1, 0, 'y'],
			[2, 0, '']
		]
	);
	assert.equal(moved[1].name, 'b');
	const current = new Map([
		[1, 'new'],
		[2, 'new']
	]);
	const rolled = restoreOverrides(current, new Map([[1, undefined], [2, 'old']]));
	assert.deepEqual([...rolled], [[2, 'old']]);
	assert.deepEqual([...current], [[1, 'new'], [2, 'new']], 'the current map is left as it was');
});

test('toggled adds or removes, on a copy', () => {
	const base = new Set(['a']);
	assert.deepEqual([...toggled(base, 'b')], ['a', 'b']);
	assert.deepEqual([...toggled(base, 'a')], []);
	assert.deepEqual([...base], ['a']);
});

test('a save echoes every field the server does not keep by itself, patched', () => {
	const payload = targetToPayload(t(5, 'nas', { tags: { env: 'prod' }, group_name: 'Storage' }), { enabled: false });
	assert.deepEqual(payload, {
		name: 'nas',
		address: 'nas.lan',
		kind: 'snmp',
		parent_id: null,
		via_agent: null,
		interval_secs: 60,
		enabled: false,
		tags: { env: 'prod' },
		group_name: 'Storage'
	});
});
