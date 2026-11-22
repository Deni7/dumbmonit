// Public status page language: the page default only applies to a browser speaking none of ours. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { browserSpeaksOurs, fallbackLocale } from '../src/lib/components/status/public-locale.ts';

const locales = ['en', 'de', 'es', 'fr', 'it', 'pt', 'pt-BR', 'ru', 'zh-Hans'];
const base = { stored: null, languages: ['ja-JP', 'ko'], locales, current: 'en', defaultLang: 'fr' };

test('a regional browser tag counts as ours', () => {
	assert.equal(browserSpeaksOurs(['fr-CA'], locales), true);
	assert.equal(browserSpeaksOurs(['zh-CN'], locales), true);
	assert.equal(browserSpeaksOurs(['ja'], locales), false);
	assert.equal(browserSpeaksOurs([], locales), false);
});

test('an unsupported browser gets the page default', () => {
	assert.equal(fallbackLocale(base), 'fr');
});

test('a stored choice or a supported browser language wins over the page default', () => {
	assert.equal(fallbackLocale({ ...base, stored: 'de' }), null);
	assert.equal(fallbackLocale({ ...base, languages: ['es-MX'] }), null);
});

test('no switch when the default is empty, unknown or already in use', () => {
	assert.equal(fallbackLocale({ ...base, defaultLang: '' }), null);
	assert.equal(fallbackLocale({ ...base, defaultLang: undefined }), null);
	assert.equal(fallbackLocale({ ...base, defaultLang: 'xx' }), null);
	assert.equal(fallbackLocale({ ...base, current: 'fr' }), null);
});
