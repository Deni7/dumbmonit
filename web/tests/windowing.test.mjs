// Windowing math of the compact monitors list (`devices/windowing.ts`).
// Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	OVERSCAN_ROWS,
	ROW_REM,
	WINDOW_MIN_ROWS,
	rowHeightPx,
	shouldWindow,
	visibleRange
} from '../src/lib/components/devices/windowing.ts';

const base = { count: 5000, rowHeight: 62, viewportHeight: 800, overscan: 10 };

test('at the top of the page, a screenful plus the overscan below', () => {
	const range = visibleRange({ ...base, listTop: 200 });
	// 600 px of the viewport show the list: 10 rows (the 10th cut), then 10 more.
	assert.equal(range.start, 0);
	assert.equal(range.end, 20);
	assert.equal(range.padTop, 0);
	assert.equal(range.padBottom, (5000 - 20) * 62);
});

test('scrolled into the list, the overscan goes both ways', () => {
	const range = visibleRange({ ...base, listTop: -62 * 1000 });
	assert.equal(range.start, 990);
	assert.equal(range.end, 1000 + Math.ceil(800 / 62) + 10);
	assert.equal(range.padTop, 990 * 62);
	// Padding plus drawn rows always add up to the whole list.
	assert.equal(range.padTop + (range.end - range.start) * 62 + range.padBottom, 5000 * 62);
});

test('a row cut at the top edge is still drawn', () => {
	const range = visibleRange({ ...base, overscan: 0, listTop: -62 * 10 - 30 });
	assert.equal(range.start, 10);
});

test('a list far below or far above the viewport draws only its overscan, or nothing', () => {
	const below = visibleRange({ ...base, listTop: 5000 });
	assert.equal(below.start, 0);
	assert.equal(below.end, 0);
	const above = visibleRange({ ...base, listTop: -62 * 6000 });
	assert.deepEqual([above.start, above.end, above.padBottom], [5000, 5000, 0]);
	assert.equal(above.padTop, 5000 * 62);
	const justAbove = visibleRange({ ...base, listTop: -62 * 5000 + 100 });
	assert.equal(justAbove.end, 5000);
	assert.ok(justAbove.start <= 4999 - 10 + 1);
});

test('the end of the list clamps, and an empty list draws nothing', () => {
	const last = visibleRange({ ...base, count: 30, listTop: -62 * 25 });
	assert.equal(last.end, 30);
	assert.equal(last.padBottom, 0);
	assert.deepEqual(visibleRange({ ...base, count: 0, listTop: 0 }), { start: 0, end: 0, padTop: 0, padBottom: 0 });
	assert.deepEqual(visibleRange({ ...base, rowHeight: 0, listTop: 0 }), { start: 0, end: 0, padTop: 0, padBottom: 0 });
});

test('row height follows the root font size', () => {
	assert.equal(rowHeightPx(16), ROW_REM * 16);
	assert.equal(rowHeightPx(20), ROW_REM * 20);
	assert.equal(rowHeightPx(Number.NaN), ROW_REM * 16);
});

test('only a long compact list is windowed, never while reordering', () => {
	assert.equal(shouldWindow(WINDOW_MIN_ROWS, true, false), true);
	assert.equal(shouldWindow(WINDOW_MIN_ROWS - 1, true, false), false);
	assert.equal(shouldWindow(5000, false, false), false, 'tiles');
	assert.equal(shouldWindow(5000, true, true), false, 'a drag needs every row');
	assert.ok(OVERSCAN_ROWS > 0);
});
