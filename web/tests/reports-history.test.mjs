// Reports history: result words, filters by type and result, retry and PDF rules, duplication. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canDownloadPdf, canRetry, duplicateDraft, filterHistory, outcomeOf } from '../src/lib/components/reports/history.ts';

const rows = [
	{ id: 1, name: 'Weekly', kind: 'digest', last_sent_at: '2026-11-16T08:00:00Z', last_error: null },
	{ id: 2, name: 'Monthly SLA', kind: 'sla', last_sent_at: '2026-11-01T08:00:00Z', last_error: 'SMTP refused' },
	{ id: 3, name: 'New SLA', kind: 'sla', last_sent_at: null, last_error: null },
	{ id: 4, name: 'Sent SLA', kind: 'sla', last_sent_at: '2026-10-01T08:00:00Z', last_error: null }
];

test('a report is failed, sent or waiting', () => {
	assert.deepEqual(rows.map(outcomeOf), ['sent', 'failed', 'waiting', 'sent']);
});

test('the history filters by type and result', () => {
	assert.equal(filterHistory(rows, 'all', 'all').length, 4);
	assert.deepEqual(filterHistory(rows, 'sla', 'all').map((r) => r.id), [2, 3, 4]);
	assert.deepEqual(filterHistory(rows, 'all', 'failed').map((r) => r.id), [2]);
	assert.deepEqual(filterHistory(rows, 'sla', 'sent').map((r) => r.id), [4]);
	assert.deepEqual(filterHistory(rows, 'digest', 'waiting'), []);
});

test('only a failed report is retried and only a sent SLA report has a PDF', () => {
	assert.deepEqual(rows.map(canRetry), [false, true, false, false]);
	assert.deepEqual(rows.map(canDownloadPdf), [false, true, false, true]);
});

test('a copy keeps the settings and drops the history', () => {
	const source = {
		...rows[1],
		enabled: true,
		frequency: 'monthly',
		weekday: 0,
		day_of_month: 1,
		hour: 8,
		timezone: 'UTC',
		recipients: ['ops@example.org'],
		channel_id: null,
		attach_pdf: true,
		attach_csv: false,
		page_id: null,
		objective: 99.9,
		next_run_at: '2026-12-01T08:00:00Z',
		layout: { notes: 'x', objectives: { groups: { Edge: 99 }, monitors: {} } },
		scope: { targets: [1], groups: [] }
	};
	const copy = duplicateDraft(source, 'Monthly SLA (copy)');
	assert.equal(copy.name, 'Monthly SLA (copy)');
	assert.equal(copy.hour, 8);
	assert.equal(copy.kind, 'sla');
	assert.deepEqual(copy.layout, source.layout);
	assert.notEqual(copy.layout, source.layout, 'a deep copy');
	assert.notEqual(copy.recipients, source.recipients);
	for (const gone of ['id', 'last_sent_at', 'last_error', 'next_run_at']) assert.equal(gone in copy, false, gone);
});
