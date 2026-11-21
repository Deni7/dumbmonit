// Post-mortem drafts: prefilled timeline, what is sent, when it can be published. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cleanDraft, paragraphs, publishBlock, startDraft, timelineFromUpdates } from '../src/lib/components/status/postmortem.ts';

const updates = [
	{ created_at: '2026-11-01T10:30:00Z', body: 'Disk replaced.' },
	{ created_at: '2026-11-01 10:00:00', body: ' Looking into it. ' },
	{ created_at: '2026-11-01T10:10:00', body: '   ' }
];

test('the timeline starts from the updates, oldest first, in UTC', () => {
	const timeline = timelineFromUpdates(updates);
	assert.deepEqual(timeline, [
		{ at: '2026-11-01T10:00:00Z', text: 'Looking into it.' },
		{ at: '2026-11-01T10:30:00Z', text: 'Disk replaced.' }
	]);
});

test('a saved post-mortem opens as saved, a new one from the updates', () => {
	const fresh = startDraft(null, updates);
	assert.equal(fresh.published, false);
	assert.equal(fresh.timeline.length, 2);
	const saved = {
		incident_id: 1,
		summary: 'S',
		impact: '',
		root_cause: '',
		timeline: [{ at: '2026-11-01T09:00:00Z', text: 'Paged' }],
		actions: [{ text: 'Spare', done: true }],
		published: true,
		published_at: '2026-11-02T08:00:00Z',
		created_at: '',
		updated_at: ''
	};
	const opened = startDraft(saved, updates);
	assert.deepEqual(opened.timeline, saved.timeline);
	opened.timeline[0].text = 'changed';
	assert.equal(saved.timeline[0].text, 'Paged', 'the draft is a copy');
});

test('empty rows are not sent and the timeline is sorted', () => {
	const sent = cleanDraft({
		summary: ' Summary ',
		impact: '',
		root_cause: '',
		timeline: [
			{ at: '2026-11-01T11:00:00Z', text: 'Later' },
			{ at: '2026-11-01T10:00:00Z', text: 'Earlier' },
			{ at: '2026-11-01T12:00:00Z', text: '  ' }
		],
		actions: [{ text: ' Do it ', done: false }, { text: '', done: true }],
		published: false
	});
	assert.equal(sent.summary, 'Summary');
	assert.deepEqual(sent.timeline.map((e) => e.text), ['Earlier', 'Later']);
	assert.deepEqual(sent.actions, [{ text: 'Do it', done: false }]);
});

test('publishing waits for the resolution and a summary', () => {
	const draft = startDraft(null, []);
	assert.equal(publishBlock('monitoring', { ...draft, summary: 'x' }), 'not_resolved');
	assert.equal(publishBlock('resolved', draft), 'no_summary');
	assert.equal(publishBlock('resolved', { ...draft, summary: 'x' }), null);
});

test('paragraphs split on blank lines', () => {
	assert.deepEqual(paragraphs('One.\nStill one.\n\n  Two.\n\n\n'), ['One.\nStill one.', 'Two.']);
	assert.deepEqual(paragraphs('   '), []);
});
