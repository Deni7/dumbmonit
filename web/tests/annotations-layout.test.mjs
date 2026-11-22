// Where a change marker sits over a chart: pure positioning math. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { markerSpan, snapRange, visibleRange } from '../src/lib/annotations/layout.ts';

// A plot that shows seconds 1000..2000 across 500 px: half a pixel per second.
const place = (at, until) => markerSpan(at, until, 1000, 2000, 500);

test('an instant lands proportionally across the plot', () => {
	assert.deepEqual(place(1000), { x: 0, width: 0, clippedStart: false, clippedEnd: false });
	assert.deepEqual(place(1500), { x: 250, width: 0, clippedStart: false, clippedEnd: false });
	assert.equal(place(2000)?.x, 500, 'the right edge is inside');
});

test('an instant outside the visible range is not drawn', () => {
	assert.equal(place(999), null);
	assert.equal(place(2001), null);
});

test('a window is a band from its start to its end', () => {
	assert.deepEqual(place(1200, 1400), { x: 100, width: 100, clippedStart: false, clippedEnd: false });
});

test('a window that began before the range is clipped to the left edge', () => {
	assert.deepEqual(place(500, 1200), { x: 0, width: 100, clippedStart: true, clippedEnd: false });
});

test('a window that ends after the range is clipped to the right edge', () => {
	assert.deepEqual(place(1800, 5000), { x: 400, width: 100, clippedStart: false, clippedEnd: true });
});

test('a window covering the whole range fills the plot', () => {
	assert.deepEqual(place(0, 9000), { x: 0, width: 500, clippedStart: true, clippedEnd: true });
});

test('a window entirely outside the range is not drawn', () => {
	assert.equal(place(100, 900), null);
	assert.equal(place(2100, 2200), null);
});

test('a window that ends exactly where the range starts still touches it', () => {
	assert.deepEqual(place(800, 1000), { x: 0, width: 0, clippedStart: true, clippedEnd: false });
});

test('an end that is not after the start is an instant', () => {
	assert.equal(place(1500, 1500)?.width, 0);
	assert.equal(place(1500, 1400)?.width, 0);
	assert.equal(place(1500, undefined)?.width, 0);
});

test('nothing is drawn on a plot without extent or with a broken range', () => {
	assert.equal(markerSpan(1500, undefined, 1000, 2000, 0), null);
	assert.equal(markerSpan(1500, undefined, 2000, 2000, 500), null);
	assert.equal(markerSpan(1500, undefined, 2000, 1000, 500), null);
	assert.equal(markerSpan(Number.NaN, undefined, 1000, 2000, 500), null);
	assert.equal(markerSpan(1500, undefined, -Infinity, Infinity, 500), null);
});

test('the visible range is the window up to now', () => {
	assert.deepEqual(visibleRange(10_000, 3600), { from: 6400, to: 10_000 });
});

test('a range snaps outward, so a few seconds of drift ask for the same window', () => {
	assert.deepEqual(snapRange(6401, 10_001), { from: 6360, to: 10_020 });
	assert.deepEqual(snapRange(6401, 10_019), snapRange(6419, 10_001));
});
