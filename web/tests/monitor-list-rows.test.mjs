// Monitors list: compact age, tag picker, location badge code, table columns
// and sub-monitors folded inside their parent.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ageParts } from '../src/lib/components/devices/age.ts';
import { tagChoices } from '../src/lib/components/devices/listState.ts';
import { defaultLocationCode, normalizeLocationCode } from '../src/lib/components/settings/locationCode.ts';
import { buildRack } from '../src/lib/components/devices/rack.ts';
import { foldRack, markContext, nestSorted, withAncestors } from '../src/lib/components/devices/nesting.ts';

test('the age is the largest whole unit, rounded down', () => {
	assert.deepEqual(ageParts(0), { n: 0, unit: 's' });
	assert.deepEqual(ageParts(59.9), { n: 59, unit: 's' });
	assert.deepEqual(ageParts(60), { n: 1, unit: 'min' });
	assert.deepEqual(ageParts(125), { n: 2, unit: 'min' });
	assert.deepEqual(ageParts(3599), { n: 59, unit: 'min' });
	assert.deepEqual(ageParts(3 * 3600 + 10), { n: 3, unit: 'h' });
	assert.deepEqual(ageParts(24 * 3600), { n: 1, unit: 'd' });
	assert.deepEqual(ageParts(5 * 86400 + 4000), { n: 5, unit: 'd' });
	assert.deepEqual(ageParts(-30), { n: 0, unit: 's' }, 'a clock ahead of the server never shows a negative age');
});

test('the tag picker keeps chosen tags, even unused, and narrows by what is typed', () => {
	const counts = [['env=prod', 4], ['db', 2], ['env=dev', 1]];
	assert.deepEqual(tagChoices(counts, [], ''), counts);
	assert.deepEqual(tagChoices(counts, ['gone'], ''), [['gone', 0], ...counts], 'a stale tag stays so it can be cleared');
	assert.deepEqual(tagChoices(counts, [], ' ENV '), [['env=prod', 4], ['env=dev', 1]]);
	assert.deepEqual(tagChoices(counts, [], 'zzz'), []);
});

test('the location code is two upper-case letters or digits, accents removed', () => {
	assert.equal(defaultLocationCode('Crissier'), 'CR');
	assert.equal(defaultLocationCode('Éclair'), 'EC');
	assert.equal(defaultLocationCode('  zürich'), 'ZU');
	assert.equal(defaultLocationCode('A'), 'A');
	assert.equal(defaultLocationCode('--'), '');
	assert.equal(normalizeLocationCode('lausanne'), 'LA');
	assert.equal(normalizeLocationCode('é7x'), 'E7');
});

test('every locale has the new keys of the list', () => {
	for (const locale of ['en', 'fr', 'de', 'es', 'it', 'pt', 'pt-BR', 'ru', 'zh-Hans']) {
		const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
		for (const key of ['locations_code', 'devices_list_bulk_more', 'devices_targets_all_tags', 'devices_targets_tags_search', 'devices_targets_tags_none', 'devices_targets_tags_more', 'devices_targets_tags_clear']) {
			assert.ok(messages[key], `${locale}.${key}`);
		}
	}
});

test('every locale has the column headers of the monitors table', () => {
	for (const locale of ['en', 'fr', 'de', 'es', 'it', 'pt', 'pt-BR', 'ru', 'zh-Hans']) {
		const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
		for (const key of ['name', 'site', 'last24h', 'security', 'checked']) {
			assert.ok(messages[`devices_cols_${key}`], `${locale}.devices_cols_${key}`);
		}
		assert.equal(messages.devices_cols_response, undefined, `${locale}: the response column is gone`);
		for (const key of ['devices_rack_children', 'devices_rack_children_trouble']) {
			assert.ok(messages[key], `${locale}.${key}`);
		}
	}
});

test('the table grid and its hidden cells fold on the same breakpoints', () => {
	const columns = readFileSync(new URL('../src/lib/components/devices/columns.ts', import.meta.url), 'utf8');
	// Visible columns: mark, name (alerts and sub-monitors inline) and checked
	// narrow; + site and the bars from @xl; + security from @3xl and @5xl (only
	// the bars widen). No response nor alert column any more.
	const counts = [...columns.matchAll(/grid-cols-\[([^\]]+)\]/g)].map((m) => m[1].split('_').length);
	assert.deepEqual(counts, [3, 5, 6, 6]);
	assert.doesNotMatch(columns, /response/);
});

const mon = (id, name, parent_id = null, extra = {}) => ({
	id,
	name,
	address: '',
	kind: 'agent',
	parent_id,
	position: 0,
	group_name: '',
	enabled: true,
	tags: {},
	last_probe_at: null,
	...extra
});
const ids = (rows) => rows.map((r) => r.target.id);

test('sub-monitors are folded inside their parent, with its count of direct children', () => {
	const fleet = [mon(1, 'host'), mon(2, 'docker', 1), mon(3, 'zfs', 1), mon(4, 'web', 2), mon(5, 'other')];
	const rows = buildRack(fleet, () => 'online', new Set(fleet.map((t) => t.id)));
	const closed = foldRack(rows, () => false);
	assert.deepEqual(ids(closed), [1, 5]);
	assert.equal(closed[0].childCount, 2, 'docker and zfs, not the grandchild');
	assert.equal(closed[0].open, false);
	assert.equal(closed[1].childCount, 0);

	const hostOnly = foldRack(rows, (id) => id === 1);
	assert.deepEqual(ids(hostOnly), [1, 2, 3, 5], 'docker stays closed over web');
	assert.equal(hostOnly.find((r) => r.target.id === 2).childCount, 1);
	assert.deepEqual(ids(foldRack(rows, () => true)), [1, 2, 4, 3, 5]);
});

test('trouble below a parent is counted, so it can open by itself', () => {
	const fleet = [mon(1, 'host'), mon(2, 'docker', 1)];
	const states = new Map([[1, 'online'], [2, 'down']]);
	const rows = buildRack(fleet, (t) => states.get(t.id), new Set([1, 2]));
	const seen = [];
	const folded = foldRack(rows, (id, trouble) => {
		seen.push([id, trouble]);
		return trouble > 0;
	});
	assert.deepEqual(seen, [[1, 1]]);
	assert.deepEqual(ids(folded), [1, 2]);
});

test('a matching sub-monitor brings its parents along, flagged as context', () => {
	const fleet = [mon(1, 'host'), mon(2, 'docker', 1), mon(3, 'web', 2), mon(4, 'other')];
	const matching = new Set([3]);
	const listed = withAncestors(fleet, matching, new Set([1, 2, 3, 4]));
	assert.deepEqual([...listed].sort(), [1, 2, 3]);
	const rows = markContext(buildRack(fleet, () => 'online', listed), matching);
	assert.deepEqual(rows.map((r) => [r.target.id, r.depth, Boolean(r.context)]), [[1, 0, true], [2, 1, true], [3, 2, false]]);
	// A parent from another section (disabled) is not pulled in.
	assert.deepEqual([...withAncestors(fleet, matching, new Set([2, 3]))].sort(), [2, 3]);
});

test('a sorted list keeps its order among siblings, children under their parent', () => {
	const sortedFlat = [mon(3, 'a-child', 2), mon(1, 'b-root'), mon(2, 'c-host'), mon(4, 'd-child', 2)].map((target) => ({
		target,
		state: 'online',
		depth: 0,
		shadowed: false
	}));
	const nested = nestSorted(sortedFlat);
	assert.deepEqual(nested.map((r) => [r.target.id, r.depth]), [[1, 0], [2, 0], [3, 1], [4, 1]]);
});
