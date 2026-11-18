// SLA report options: query string of GET /api/reports/sla, read back by the printable page,
// template defaults and the chart scale. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	REPORT_SECTION_KEYS,
	chartFloor,
	chartShare,
	defaultReportLayout,
	normaliseLayout,
	sectionsFor,
	slaParams,
	slaRequestFrom
} from '../src/lib/sla-report.ts';

test('the summary template starts without the long tables', () => {
	const summary = sectionsFor('summary');
	assert.equal(summary.summary, true);
	assert.equal(summary.incidents, true);
	assert.equal(summary.maintenance, false);
	assert.equal(summary.response_times, false);
	const detailed = sectionsFor('detailed');
	assert.ok(REPORT_SECTION_KEYS.every((key) => detailed[key]));
});

test('an old schedule without a layout reads as the detailed report', () => {
	assert.deepEqual(normaliseLayout(undefined), defaultReportLayout());
	const partial = normaliseLayout({ template: 'summary', sections: { chart: false } });
	assert.equal(partial.template, 'summary');
	assert.equal(partial.sections.chart, false);
	assert.equal(partial.sections.notes, true);
	assert.equal(partial.show_logo, true);
	assert.equal(normaliseLayout({ paper: 'tabloid' }).paper, 'a4');
});

test('the layout and the scope travel in the query string', () => {
	const layout = { ...defaultReportLayout(), template: 'summary', paper: 'letter', title: ' Board pack ', notes: 'Quiet.', show_logo: false };
	layout.sections = { ...sectionsFor('summary'), chart: false };
	const params = slaParams({
		from: '2026-10-01',
		to: '2026-10-31',
		format: 'pdf',
		objective: 99.9,
		page: undefined,
		layout,
		scope: { targets: [3, 7], groups: ['Edge, core'] }
	});
	assert.equal(params.get('format'), 'pdf');
	assert.equal(params.get('objective'), '99.9');
	assert.equal(params.has('page'), false, 'unset values are left out');
	assert.equal(params.get('template'), 'summary');
	assert.equal(params.get('paper'), 'letter');
	assert.equal(params.get('sections'), 'summary,monitors,incidents,notes');
	assert.equal(params.get('title'), 'Board pack');
	assert.equal(params.get('notes'), 'Quiet.');
	assert.equal(params.get('logo'), 'false');
	assert.deepEqual(JSON.parse(params.get('scope')), { targets: [3, 7], groups: ['Edge, core'] });

	const back = slaRequestFrom(params);
	assert.equal(back.format, 'json');
	assert.equal(back.from, '2026-10-01');
	assert.equal(back.objective, 99.9);
	assert.equal(back.page, undefined);
	assert.equal(back.layout.template, 'summary');
	assert.equal(back.layout.paper, 'letter');
	assert.equal(back.layout.sections.chart, false);
	assert.equal(back.layout.sections.maintenance, false);
	assert.equal(back.layout.title, 'Board pack');
	assert.equal(back.layout.show_logo, false);
	assert.deepEqual(back.scope, { targets: [3, 7], groups: ['Edge, core'] });
});

test('an empty scope and the default logo add nothing to the address', () => {
	const params = slaParams({ format: 'csv', layout: defaultReportLayout(), scope: { targets: [], groups: [] } });
	assert.equal(params.has('scope'), false);
	assert.equal(params.has('logo'), false);
	assert.equal(params.has('title'), false);
	// No section ticked would read as "every section": the summary is kept.
	const none = { ...defaultReportLayout(), sections: Object.fromEntries(REPORT_SECTION_KEYS.map((k) => [k, false])) };
	assert.equal(slaParams({ format: 'pdf', layout: none }).get('sections'), 'summary');
});

test('a broken scope in the address falls back to every monitor', () => {
	const back = slaRequestFrom(new URLSearchParams('scope=%7Bnope&objective=abc'));
	assert.deepEqual(back.scope, { targets: [], groups: [] });
	assert.equal(back.objective, undefined);
	assert.deepEqual(back.layout, defaultReportLayout());
});

test('the chart scale starts below the lowest value and the objective', () => {
	assert.equal(chartFloor([99.95, 99.99], null), 99.9);
	assert.equal(chartFloor([99.95], 99.9), 99.5);
	assert.equal(chartFloor([98.5], 99.9), 98);
	assert.equal(chartFloor([12], null), 0);
	assert.equal(chartFloor([], null), 99.9);
	assert.equal(chartShare(100, 98), 1);
	assert.equal(chartShare(99, 98), 0.5);
	assert.equal(chartShare(90, 98), 0, 'below the floor sits on the axis');
});
