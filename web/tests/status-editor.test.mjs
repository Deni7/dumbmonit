// Status page editor: snippets to show a page elsewhere and "unsaved changes" (order: status-structure.test.mjs). Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	badgeHtml,
	badgeMarkdown,
	badgeUrl,
	bannerHtml,
	bannerPhp,
	embedIframe,
	embedUrl,
	publicApi,
	serviceKeys,
	slugify
} from '../src/lib/components/status/share.ts';
import { formFingerprint } from '../src/lib/components/status/form-state.ts';

const O = 'https://mon.example';

test('slugify follows the server rule', () => {
	assert.equal(slugify('  Home Lab — Paris!  '), 'home-lab-paris');
	assert.equal(slugify('a'.repeat(50)).length, 40);
});

test('service keys are unique and never empty', () => {
	assert.deepEqual(
		serviceKeys(['Web', 'Web', '!!!']).map((c) => c.key),
		['web', 'web-2', 'service']
	);
});

test('badge addresses per kind, page or service', () => {
	assert.equal(badgeUrl(O, 'lab', 'badge'), `${O}/api/public/status/lab/badge.svg`);
	assert.equal(badgeUrl(O, 'lab', 'uptime', '', 7), `${O}/api/public/status/lab/uptime.svg?days=7`);
	assert.equal(badgeUrl(O, 'lab', 'response', 'web'), `${O}/api/public/status/lab/components/web/response.svg`);
});

test('badge snippets link the image to the page and escape the alt text', () => {
	assert.equal(badgeMarkdown('Lab status', 'i.svg', 'p'), '[![Lab status](i.svg)](p)');
	assert.equal(badgeHtml('"Lab" status', 'i.svg', 'p'), '<a href="p"><img src="i.svg" alt="&quot;Lab&quot; status"></a>');
});

test('embed address only carries what differs from the page', () => {
	assert.equal(embedUrl(O, 'lab', 'page', true), `${O}/s/lab/embed`);
	assert.equal(embedUrl(O, 'lab', 'dark', true), `${O}/s/lab/embed?theme=dark`);
	assert.equal(embedUrl(O, 'lab', 'light', false), `${O}/s/lab/embed?theme=light&history=0`);
	assert.match(embedIframe('u', 'A <b> status'), /title="A &lt;b&gt; status"/);
});

test('website banner snippets point at the public API of the page', () => {
	const api = publicApi(O, 'lab');
	assert.equal(api, `${O}/api/public/status/lab`);
	assert.match(bannerHtml(api, 'fr'), /src="https:\/\/mon\.example\/api\/public\/status\/lab\/banner\.js"/);
	assert.match(bannerHtml(api, 'fr'), /data-lang="fr"><\/script>$/);
	assert.match(bannerPhp(api), /dumbmonit_banner\('https:\/\/mon\.example\/api\/public\/status\/lab',/);
});

test('unsaved changes ignore surrounding spaces but see real edits', () => {
	const base = { title: 'Lab', items: [{ label: 'Web' }], published: false };
	assert.equal(formFingerprint(base), formFingerprint({ ...base, title: 'Lab  ' }));
	assert.notEqual(formFingerprint(base), formFingerprint({ ...base, published: true }));
	assert.notEqual(formFingerprint(base), formFingerprint({ ...base, items: [{ label: 'Site' }] }));
});
