import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';

const LOCALES = ['en', 'de', 'es', 'fr', 'it', 'pt', 'pt-BR', 'ru', 'zh-Hans'];
const source = readFileSync(new URL('../src/lib/components/settings/auditActions.ts', import.meta.url), 'utf8');

/** `messages/en.json`, plus the fragments not merged yet. */
function messages(locale) {
	const all = {};
	const dir = new URL('../messages/', import.meta.url);
	Object.assign(all, JSON.parse(readFileSync(new URL(`${locale}.json`, dir), 'utf8')));
	const fragments = new URL('fragments/', dir);
	if (existsSync(fragments)) {
		for (const file of readdirSync(fragments)) {
			if (file.endsWith(`.${locale}.json`)) {
				Object.assign(all, JSON.parse(readFileSync(new URL(file, fragments), 'utf8')));
			}
		}
	}
	return all;
}

const used = [...source.matchAll(/m\.(w2ops_audit_action_[a-z_0-9]+)\(\)/g)].map((match) => match[1]);

test('the audit labels table is not empty and has no duplicate action', () => {
	assert.ok(used.length > 100, `${used.length} labels`);
	const actions = [...source.matchAll(/^\t'([a-z_0-9.]+)':/gm)].map((match) => match[1]);
	assert.equal(new Set(actions).size, actions.length);
});

for (const locale of LOCALES) {
	test(`every audit label has a text in ${locale}`, () => {
		const texts = messages(locale);
		for (const key of used) {
			assert.ok(typeof texts[key] === 'string' && texts[key].length > 0, `${locale}: ${key}`);
		}
	});
}

test('every action the server records has a label', () => {
	// `audit::record(..., "action", ...)` and the literals returned for the dynamic ones.
	const root = new URL('../../crates/server/src/', import.meta.url);
	if (!existsSync(root)) return;
	const found = new Set();
	const walk = (dir) => {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			const path = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
			if (entry.isDirectory()) walk(path);
			else if (entry.name.endsWith('.rs')) {
				const text = readFileSync(path, 'utf8');
				for (const call of text.matchAll(/audit::record\(([^;]*?)\)\s*\.await/gs)) {
					const literal = call[1].match(/"((?:[a-z_0-9]+\.)+[a-z_0-9]+|login|logout)"/);
					if (literal) found.add(literal[1]);
				}
			}
		}
	};
	walk(root);
	const labelled = new Set([...source.matchAll(/^\t'([a-z_0-9.]+)':/gm)].map((match) => match[1]));
	const missing = [...found].filter((action) => !labelled.has(action));
	assert.deepEqual(missing, []);
});
