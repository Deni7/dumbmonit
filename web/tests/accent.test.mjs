// The instance accent: any hue gets a readable ink and a fill that stands out in both lights. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ACCENT_PALETTE, contrast, deriveAccent, inkOn, parseAccent } from '../src/lib/accent.ts';

test('only #rrggbb is an accent; the rest is the default look', () => {
	assert.equal(parseAccent('#2563EB'), '#2563eb');
	assert.equal(parseAccent(' #abcdef '), '#abcdef');
	for (const bad of ['', 'blue', '#fff', '#12345g', '2563eb', null, undefined]) assert.equal(parseAccent(bad), '');
	assert.equal(deriveAccent(''), null);
});

test('contrast matches the WCAG reference values', () => {
	assert.equal(Math.round(contrast('#000000', '#ffffff')), 21);
	assert.equal(contrast('#777777', '#777777'), 1);
});

test('every palette hue and a sweep of colours keep 4.5:1 ink and 3:1 fill', () => {
	const hues = ACCENT_PALETTE.map((p) => p.hex).filter(Boolean);
	for (let r = 0; r < 256; r += 51) for (let g = 0; g < 256; g += 51) for (let b = 0; b < 256; b += 51) {
		hues.push('#' + [r, g, b].map((c) => c.toString(16).padStart(2, '0')).join(''));
	}
	for (const hex of hues) {
		const d = deriveAccent(hex);
		assert.ok(contrast(d.lightFill, d.lightOn) >= 4.5, `light ink on ${hex}`);
		assert.ok(contrast(d.darkFill, d.darkOn) >= 4.5, `dark ink on ${hex}`);
		assert.ok(contrast(d.lightFill, '#ffffff') >= 3, `light fill ${hex}`);
		assert.ok(contrast(d.darkFill, '#0b0d10') >= 3, `dark fill ${hex}`);
	}
});

test('a pale hue is darkened on light, a deep one lightened on dark, the hue kept', () => {
	const pale = deriveAccent('#ffe066');
	assert.notEqual(pale.lightFill, '#ffe066');
	assert.equal(pale.darkFill, '#ffe066');
	const deep = deriveAccent('#10205a');
	assert.notEqual(deep.darkFill, '#10205a');
	assert.equal(inkOn('#ffffff'), '#07080a');
	assert.equal(inkOn('#000000'), '#ffffff');
});
