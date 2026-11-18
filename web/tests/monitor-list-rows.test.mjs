// Monitors list: compact age, tag picker and location badge code.
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ageParts } from '../src/lib/components/devices/age.ts';
import { tagChoices } from '../src/lib/components/devices/listState.ts';
import { defaultLocationCode, normalizeLocationCode } from '../src/lib/components/settings/locationCode.ts';

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
