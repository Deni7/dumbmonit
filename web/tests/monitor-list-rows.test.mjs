// Monitors list: compact age, tag picker, location badge code, table columns
// (named once over the list) and sub-monitors folded inside their parent.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ageParts } from '../src/lib/components/devices/age.ts';
import { tagChoices } from '../src/lib/components/devices/listState.ts';
import { defaultLocationCode, normalizeLocationCode } from '../src/lib/components/settings/locationCode.ts';
import { buildRack } from '../src/lib/components/devices/rack.ts';
import { foldRack, hostOf, inferHosts, markContext, nestSorted, ownName, withAncestors } from '../src/lib/components/devices/nesting.ts';
import { gridFor, SHOW, STATE_GLYPH } from '../src/lib/components/devices/columns.ts';

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
		for (const key of ['devices_rack_children', 'devices_rack_children_trouble', 'devices_folder_trouble']) {
			assert.ok(messages[key], `${locale}.${key}`);
		}
	}
});

test('the table grid and its hidden cells fold on the same breakpoints', () => {
	const columns = readFileSync(new URL('../src/lib/components/devices/columns.ts', import.meta.url), 'utf8');
	assert.doesNotMatch(columns, /response/);
	const tracks = (grid) => [...grid.matchAll(/(?:^|\s)(?:(@\w+):)?grid-cols-\[([^\]]+)\]/g)].map((m) => [m[1] ?? '', m[2].split('_').length]);
	// Visible columns: mark, name (alerts and sub-monitors inline) and checked
	// narrow; + site and the bars from @xl; + security from @3xl and @5xl (only
	// the bars widen). No response nor alert column any more.
	assert.deepEqual(tracks(gridFor(true)), [['', 3], ['@xl', 5], ['@3xl', 6], ['@5xl', 6]]);
	// Fewer than two locations: no site track at all, the same breakpoints otherwise.
	assert.deepEqual(tracks(gridFor(false)), [['', 3], ['@xl', 4], ['@3xl', 5], ['@5xl', 5]]);
	// The hidden cells appear where their tracks do.
	assert.equal(SHOW.site, 'hidden @xl:flex');
	assert.equal(SHOW.river, 'hidden @xl:flex');
	assert.equal(SHOW.security, 'hidden @3xl:flex');
	// Without the site column, the rows and the header drop the site cell (not just empty it).
	const row = readFileSync(new URL('../src/lib/components/devices/CompactRow.svelte', import.meta.url), 'utf8');
	const header = readFileSync(new URL('../src/lib/components/devices/ColumnHeader.svelte', import.meta.url), 'utf8');
	assert.match(row, /\{#if siteColumn\}\s*<span class=\{`\$\{SHOW\.site\}/);
	assert.match(header, /\{#if showSite\}<span class=\{`\$\{SHOW\.site\}/);
	// The column names are never cut short.
	assert.doesNotMatch(header, /truncate[^"]*">\{m\.devices_cols_(?!name)/);
});

test('the column names are written once over the list, not in every folder card', () => {
	const list = readFileSync(new URL('../src/lib/components/devices/RackList.svelte', import.meta.url), 'utf8');
	const page = readFileSync(new URL('../src/lib/components/monitors/MonitorList.svelte', import.meta.url), 'utf8');
	assert.doesNotMatch(list, /devices_cols_/, 'no header line inside a folder card');
	assert.equal(page.match(/<ColumnHeader\b/g)?.length, 1);
});

test('under its parent, a sub-monitor drops the name it repeats', () => {
	assert.equal(ownName('NAS70 · Databases', 'NAS70'), 'Databases');
	assert.equal(ownName('nas · Databases', 'NAS'), 'Databases', 'whatever the case');
	assert.equal(ownName('pve: backup', 'pve'), 'backup');
	assert.equal(ownName('nas - Docker', 'nas'), 'Docker');
	assert.equal(ownName('nas Docker', 'nas'), 'Docker');
	assert.equal(ownName('nas-01', 'nas'), 'nas-01', 'a mark glued to a word is part of the name');
	assert.equal(ownName('nasa', 'nas'), 'nasa');
	assert.equal(ownName('Postgres', 'nas'), 'Postgres');
	assert.equal(ownName('nas · ', 'nas'), 'nas · ', 'never an empty name');
	assert.equal(ownName('nas', 'nas'), 'nas');
	assert.equal(ownName('nas · Docker', null), 'nas · Docker');
});

test('every state is the same circle, told apart by its glyph as well as its tone', () => {
	assert.deepEqual(STATE_GLYPH, {
		online: 'none',
		offline: 'cross',
		down: 'cross',
		misconfigured: 'bang',
		pending: 'dot',
		disabled: 'ring',
		unknown: 'ring'
	});
	const mark = readFileSync(new URL('../src/lib/ui/StatusMark.svelte', import.meta.url), 'utf8');
	assert.doesNotMatch(mark, /<rect|<path d="M7 2/, 'no square, triangle nor diamond any more');
});

test('every locale has the kind tags of the table', () => {
	for (const locale of ['en', 'fr', 'de', 'es', 'it', 'pt', 'pt-BR', 'ru', 'zh-Hans']) {
		const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
		for (const key of ['agent', 'service', 'databases', 'speedtest', 'push', 'domain', 'blocklist', 'reputation', 'netconfig', 'website', 'webchange', 'demo']) {
			assert.ok(messages[`devices_kindtag_${key}`], `${locale}.devices_kindtag_${key}`);
		}
	}
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

test('the host of an address or a URL', () => {
	assert.equal(hostOf('db.home.lan'), 'db.home.lan');
	assert.equal(hostOf('DB.Home.Lan:5432'), 'db.home.lan');
	assert.equal(hostOf('postgres://app:secret@db.lan:5432/app?sslmode=off'), 'db.lan');
	assert.equal(hostOf('https://nas.lan:5001/health'), 'nas.lan');
	assert.equal(hostOf('192.168.1.10'), '192.168.1.10');
	assert.equal(hostOf('192.168.1.10:6379'), '192.168.1.10');
	assert.equal(hostOf('[fd00::5]:443'), 'fd00::5');
	assert.equal(hostOf('fd00::5'), 'fd00::5');
	assert.equal(hostOf('nas.'), 'nas');
	assert.equal(hostOf('localhost:5432'), null, 'the loopback is every machine');
	assert.equal(hostOf('127.0.0.1'), null);
	assert.equal(hostOf('[::1]:80'), null);
	assert.equal(hostOf('My NAS'), null, 'a name with spaces is no host');
	assert.equal(hostOf(''), null);
});

test('a database or a web check aimed at a server of the list goes inside it', () => {
	const fleet = [
		mon(1, 'nas', null, { kind: 'agent', address: 'machine-id-1' }),
		mon(2, 'Postgres', null, { kind: 'postgres', address: 'nas.lan:5432' }),
		mon(3, 'Redis', null, { kind: 'redis', address: '192.168.1.20' }),
		mon(4, 'pve', null, { kind: 'proxmox', address: '192.168.1.20' }),
		mon(5, 'ping pve', null, { kind: 'ping', address: '192.168.1.20' }),
		mon(6, 'Docker', 1, { kind: 'agent_service', address: 'machine-id-1#docker' }),
		mon(7, 'Site', null, { kind: 'http', address: 'https://example.com' }),
		mon(8, 'Declared', 7, { kind: 'mysql', address: 'nas.lan' })
	];
	const { targets, inferred } = inferHosts(fleet);
	const parent = (id) => targets.find((t) => t.id === id).parent_id;
	assert.equal(parent(2), 1, 'nas.lan is the agent named nas');
	assert.equal(parent(3), 4, 'the Proxmox VE at that address, the surest kind');
	assert.equal(parent(5), 4, 'a ping beside a Proxmox VE goes inside it');
	assert.equal(parent(4), null, 'the Proxmox VE has nothing surer at its address');
	assert.equal(parent(6), 1, 'a declared parent stays');
	assert.equal(parent(8), 7, 'a declared parent wins over the host');
	assert.equal(parent(7), null);
	assert.deepEqual([...inferred].sort(), [2, 3, 5]);
	assert.equal(targets[6], fleet[6], 'the others are the same objects');
	assert.equal(fleet[1].parent_id, null, 'the saved monitors are left alone');
});

test('a host shared by two machines of the same kind, or another folder, gives no parent', () => {
	const tie = [
		mon(1, 'a', null, { kind: 'snmp', address: '10.0.0.5' }),
		mon(2, 'b', null, { kind: 'snmp', address: '10.0.0.5' }),
		mon(3, 'db', null, { kind: 'postgres', address: '10.0.0.5' })
	];
	assert.equal(inferHosts(tie).inferred.size, 0);
	const folders = [
		mon(1, 'srv', null, { kind: 'agent', group_name: 'Servers' }),
		mon(2, 'db', null, { kind: 'postgres', address: 'srv', group_name: 'Databases' })
	];
	assert.equal(inferHosts(folders).inferred.size, 0, 'a monitor filed in its own folder stays there');
	const twoDomains = [mon(1, 'web.a.lan', null, { kind: 'agent' }), mon(2, 'site', null, { kind: 'http', address: 'https://web.b.lan' })];
	assert.equal(inferHosts(twoDomains).inferred.size, 0, 'web.a.lan and web.b.lan are two machines');
});

test('an inferred parent never makes a cycle', () => {
	// The ping declared its parent under the database; the database must not go under the ping.
	const fleet = [
		mon(1, 'db', null, { kind: 'postgres', address: '10.0.0.9' }),
		mon(2, 'ping', 1, { kind: 'ping', address: '10.0.0.9' })
	];
	const { targets, inferred } = inferHosts(fleet);
	assert.equal(inferred.size, 0);
	assert.equal(targets.find((t) => t.id === 1).parent_id, null);
	const self = [mon(1, 'nas', null, { kind: 'agent' }), mon(2, 'nas', null, { kind: 'agent' })];
	assert.equal(inferHosts(self).inferred.size, 0, 'a monitor never lands under itself, nor under a twin');
});

test('a monitor placed under a machine for display is not shadowed by it', () => {
	const fleet = [
		mon(1, 'nas', null, { kind: 'agent' }),
		mon(2, 'db', null, { kind: 'postgres', address: 'nas' }),
		mon(3, 'docker', 1, { kind: 'agent_service' })
	];
	const { targets, inferred } = inferHosts(fleet);
	const states = new Map([[1, 'offline'], [2, 'online'], [3, 'online']]);
	const rows = buildRack(targets, (t) => states.get(t.id), new Set([1, 2, 3]), inferred);
	assert.deepEqual(rows.map((r) => [r.target.id, r.depth, r.shadowed]), [[1, 0, false], [2, 1, false], [3, 1, true]]);
});
