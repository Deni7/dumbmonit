// Status page editor: groups and monitors, their reordering (drag and drop and
// the Move up / Move down buttons), and the live preview built from them. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	addGroup,
	addItem,
	dropGroup,
	dropItem,
	duplicateNames,
	itemsFromStructure,
	moveGroup,
	moveItem,
	removeGroup,
	removeItem,
	renameGroup,
	setItemLabel,
	structureFromItems
} from '../src/lib/components/status/structure.ts';
import { blankHistory, buildPreview, previewGroups, targetState } from '../src/lib/components/status/preview.ts';

const keys = () => {
	let n = 0;
	return () => `k${++n}`;
};
/** Names and monitor ids, to compare layouts at a glance. */
const shape = (groups) => groups.map((g) => [g.name, g.items.map((i) => i.target_id)]);

const SAVED = [
	{ target_id: 1, label: 'Web', group_name: 'Front' },
	{ target_id: 2, label: 'API', group_name: 'Front' },
	{ target_id: 3, label: 'DB', group_name: 'Back' },
	{ target_id: 4, label: 'Cache', group_name: 'Back' }
];

test('saved items become groups in the order of their first item, and back', () => {
	const groups = structureFromItems(SAVED, keys());
	assert.deepEqual(shape(groups), [
		['Front', [1, 2]],
		['Back', [3, 4]]
	]);
	assert.deepEqual(groups.map((g) => g.key), ['k1', 'k2']);
	assert.deepEqual(itemsFromStructure(groups), SAVED);
});

test('a monitor listed twice is kept once, and names are trimmed on save', () => {
	const groups = structureFromItems(
		[
			{ target_id: 1, label: 'Web', group_name: ' Front ' },
			{ target_id: 1, label: 'Again', group_name: 'Other' }
		],
		keys()
	);
	assert.deepEqual(shape(groups), [['Front', [1]]]);
	const edited = setItemLabel(renameGroup(groups, 'k1', '  Edge  '), 1, ' Site ');
	assert.deepEqual(itemsFromStructure(edited), [{ target_id: 1, label: 'Site', group_name: 'Edge' }]);
});

test('an empty group is not saved', () => {
	const groups = addGroup(structureFromItems(SAVED, keys()), 'Empty', 'e');
	assert.equal(itemsFromStructure(groups).length, 4);
	assert.deepEqual(shape(groups).at(-1), ['Empty', []]);
});

test('Move up / Move down inside a group', () => {
	const groups = structureFromItems(SAVED, keys());
	assert.deepEqual(shape(moveItem(groups, 2, -1)), [
		['Front', [2, 1]],
		['Back', [3, 4]]
	]);
	assert.deepEqual(shape(moveItem(groups, 3, 1)), [
		['Front', [1, 2]],
		['Back', [4, 3]]
	]);
});

test('Move up / Move down cross into the neighbouring group at its edge', () => {
	const groups = structureFromItems(SAVED, keys());
	assert.deepEqual(shape(moveItem(groups, 3, -1)), [
		['Front', [1, 2, 3]],
		['Back', [4]]
	]);
	assert.deepEqual(shape(moveItem(groups, 2, 1)), [
		['Front', [1]],
		['Back', [2, 3, 4]]
	]);
	// Into an empty group too.
	const withEmpty = addGroup(groups, 'Later', 'e');
	assert.deepEqual(shape(moveItem(withEmpty, 4, 1)).at(-1), ['Later', [4]]);
});

test('the first and last places stay put, and the input is never changed', () => {
	const groups = structureFromItems(SAVED, keys());
	const before = JSON.stringify(groups);
	assert.deepEqual(shape(moveItem(groups, 1, -1)), shape(groups));
	assert.deepEqual(shape(moveItem(groups, 4, 1)), shape(groups));
	assert.deepEqual(shape(moveItem(groups, 99, 1)), shape(groups));
	moveItem(groups, 3, -1);
	dropItem(groups, 1, 'k2', 0);
	assert.equal(JSON.stringify(groups), before);
});

test('groups move up and down', () => {
	const groups = structureFromItems(SAVED, keys());
	assert.deepEqual(moveGroup(groups, 'k2', -1).map((g) => g.name), ['Back', 'Front']);
	assert.deepEqual(moveGroup(groups, 'k1', -1).map((g) => g.name), ['Front', 'Back']);
	assert.deepEqual(moveGroup(groups, 'k2', 1).map((g) => g.name), ['Front', 'Back']);
});

test('dropping a monitor lands before the one it was dropped on', () => {
	const groups = structureFromItems(SAVED, keys());
	// Down inside its own group: the index is the one before the move.
	assert.deepEqual(shape(dropItem(groups, 1, 'k1', 2)), [
		['Front', [2, 1]],
		['Back', [3, 4]]
	]);
	// Up inside its own group.
	assert.deepEqual(shape(dropItem(groups, 4, 'k2', 0)), [
		['Front', [1, 2]],
		['Back', [4, 3]]
	]);
	// Into another group, in the middle and at the end.
	assert.deepEqual(shape(dropItem(groups, 1, 'k2', 1)), [
		['Front', [2]],
		['Back', [3, 1, 4]]
	]);
	assert.deepEqual(shape(dropItem(groups, 1, 'k2', 2)), [
		['Front', [2]],
		['Back', [3, 4, 1]]
	]);
	// Out-of-range index: clamped to the end. Unknown group: nothing moves.
	assert.deepEqual(shape(dropItem(groups, 1, 'k2', 99)).at(-1), ['Back', [3, 4, 1]]);
	assert.deepEqual(shape(dropItem(groups, 1, 'nope', 0)), shape(groups));
});

test('dropping a group lands before the group it was dropped on', () => {
	let groups = structureFromItems(SAVED, keys());
	groups = addGroup(groups, 'Third', 'k3');
	assert.deepEqual(dropGroup(groups, 'k1', 3).map((g) => g.name), ['Back', 'Third', 'Front']);
	assert.deepEqual(dropGroup(groups, 'k1', 2).map((g) => g.name), ['Back', 'Front', 'Third']);
	assert.deepEqual(dropGroup(groups, 'k3', 0).map((g) => g.name), ['Third', 'Front', 'Back']);
	assert.deepEqual(dropGroup(groups, 'k2', 2).map((g) => g.name), ['Front', 'Back', 'Third']);
});

test('adding a monitor already placed moves it; removing a group removes its monitors', () => {
	const groups = structureFromItems(SAVED, keys());
	assert.deepEqual(shape(addItem(groups, 'k2', { target_id: 1, label: 'Web' })), [
		['Front', [2]],
		['Back', [3, 4, 1]]
	]);
	assert.deepEqual(shape(addItem(groups, 'k1', { target_id: 9, label: 'New' }))[0], ['Front', [1, 2, 9]]);
	assert.deepEqual(shape(removeItem(groups, 3)), [
		['Front', [1, 2]],
		['Back', [4]]
	]);
	assert.deepEqual(shape(removeGroup(groups, 'k1')), [['Back', [3, 4]]]);
});

test('two groups with one name are flagged: the public page shows them as one', () => {
	const groups = renameGroup(structureFromItems(SAVED, keys()), 'k2', ' Front');
	assert.deepEqual([...duplicateNames(groups)], ['Front']);
	assert.equal(duplicateNames(structureFromItems(SAVED, keys())).size, 0);
});

// --- live preview -----------------------------------------------------------

const target = (id, name, extra = {}) => ({
	id,
	name,
	kind: 'http',
	enabled: true,
	group_name: '',
	position: 0,
	last_probe_at: '2026-11-18T10:00:00',
	last_error: null,
	error_kind: null,
	...extra
});
const TARGETS = [
	target(1, 'web-01'),
	target(2, 'api-01', { error_kind: 'down', last_error: 'timeout' }),
	target(3, 'db-01', { group_name: 'Rack', position: 2 }),
	target(4, 'cache-01', { group_name: 'Rack', position: 1 }),
	target(5, 'off', { enabled: false })
];

const draft = (groups, extra = {}) => ({
	slug: 'acme',
	title: 'Acme',
	description: '',
	theme: 'auto',
	showDays: 7,
	accent: 'default',
	showCalendar: false,
	showLatency: false,
	footerText: '',
	homepageUrl: '',
	logoUrl: null,
	subscribeEmail: false,
	hideBranding: false,
	groups,
	folders: [],
	folderLabels: {},
	...extra
});

test('a monitor not on the saved page takes its state from the monitor list', () => {
	assert.equal(targetState(TARGETS[0]), 'up');
	assert.equal(targetState(TARGETS[1]), 'down');
	assert.equal(targetState(TARGETS[4]), 'unknown');
	assert.equal(targetState(undefined), 'unknown');
	assert.equal(targetState(target(6, 'new', { last_probe_at: null })), 'unknown');
});

test('folder members follow the hand-placed monitors, by folder order, without duplicates', () => {
	const groups = structureFromItems([{ target_id: 3, label: 'Database', group_name: 'Data' }], keys());
	const rows = previewGroups(draft(groups, { folders: ['Rack'], folderLabels: { 4: ' Cache ' } }), TARGETS);
	assert.deepEqual(
		rows.map((g) => [g[0].name, g.map((r) => [r.target_id, r.label])]),
		[
			['Data', [[3, 'Database']]],
			['Rack', [[4, 'Cache']]]
		]
	);
});

test('the preview follows unsaved edits and keeps what the saved page measured', () => {
	const now = new Date('2026-11-18T12:00:00Z');
	const history = [{ date: '2026-11-17', uptime_pct: 99.5, down_minutes: 7, incidents: 0 }];
	const base = {
		page: { history_days: 1, window_secs: 86400, step_secs: 86400, subscribe_kinds: ['email', 'rss'], logo_url: null, updated_at: 'x' },
		overall: 'operational',
		generated_at: 'g',
		groups: [{ name: 'Old', items: [{ key: 'web', target_id: 1, label: 'Old name', state: 'up', history, uptime_history: 99.5 }] }],
		incidents: [{ title: 'Outage', status: 'investigating' }],
		maintenance: []
	};
	const groups = structureFromItems(
		[
			{ target_id: 1, label: 'Website', group_name: 'Public' },
			{ target_id: 2, label: '', group_name: 'Public' }
		],
		keys()
	);
	const doc = buildPreview(draft(groups, { hideBranding: true, title: ' Acme ' }), TARGETS, base, now);
	assert.equal(doc.page.title, 'Acme');
	assert.equal(doc.page.hide_branding, true);
	assert.equal(doc.groups.length, 1);
	assert.equal(doc.groups[0].name, 'Public');
	const [web, api] = doc.groups[0].items;
	// Saved monitor: its measurements, under the new public name.
	assert.equal(web.label, 'Website');
	assert.equal(web.uptime_history, 99.5);
	assert.deepEqual(web.history, history);
	// New monitor: its current state, its own name, a "no data" bar of the same length.
	assert.equal(api.label, 'api-01');
	assert.equal(api.state, 'down');
	assert.deepEqual(api.history, [{ date: '2026-11-17', uptime_pct: null, down_minutes: null, incidents: 0 }]);
	assert.equal(doc.overall, 'degraded');
	assert.deepEqual(doc.incidents, base.incidents);
	// Email subscription follows the unsaved channel choice.
	assert.deepEqual(doc.page.subscribe_kinds, ['rss']);
	assert.deepEqual(buildPreview(draft(groups, { subscribeEmail: true }), TARGETS, base, now).page.subscribe_kinds, ['email', 'rss']);
});

test('a new page previews the chosen period with no data', () => {
	const now = new Date('2026-11-18T12:30:00Z');
	const groups = structureFromItems([{ target_id: 1, label: 'Web', group_name: '' }], keys());
	const week = buildPreview(draft(groups), TARGETS, null, now);
	assert.equal(week.groups[0].items[0].history.length, 7);
	assert.equal(week.groups[0].items[0].history.at(-1).date, '2026-11-18');
	const day = buildPreview(draft(groups, { showDays: 1, showCalendar: true }), TARGETS, null, now);
	assert.equal(day.page.step_secs, 3600);
	assert.equal(day.groups[0].items[0].history.length, 24);
	assert.equal(day.groups[0].items[0].history.at(-1).date, '2026-11-18T12:00:00Z');
	assert.equal(day.calendar.length, 90);
	assert.equal(blankHistory(undefined, 3, 86400, now)[0].date, '2026-11-16');
});
