/**
 * Post-mortem drafts: the form starts from the incident's own updates, and
 * what is sent drops the rows left empty. Pure functions, run by the tests.
 * Mirrors the checks of `api/status_pages/postmortem.rs`.
 */
import type { IncidentStatus, Postmortem, PostmortemPayload, PostmortemTimelineEntry } from '../../api/types';

/** Limits of the server, repeated so the form stops before a refusal. */
export const MAX_TIMELINE = 100;
export const MAX_ACTIONS = 50;

/** A server date without suffix is UTC: `2026-11-01T10:00:00` → `2026-11-01T10:00:00Z`. */
function utc(value: string): string {
	const trimmed = value.trim().replace(' ', 'T');
	return /(Z|[+-]\d\d:?\d\d)$/.test(trimmed) ? trimmed : `${trimmed}Z`;
}

/** The incident's updates as a first timeline, oldest first. */
export function timelineFromUpdates(updates: { created_at: string; body: string }[]): PostmortemTimelineEntry[] {
	return updates
		.filter((update) => update.body.trim() !== '')
		.map((update) => ({ at: utc(update.created_at), text: update.body.trim() }))
		.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

/** What the editor opens with: the saved post-mortem, else a draft prefilled from the updates. */
export function startDraft(postmortem: Postmortem | null, updates: { created_at: string; body: string }[]): PostmortemPayload {
	if (postmortem) {
		return {
			summary: postmortem.summary,
			impact: postmortem.impact,
			root_cause: postmortem.root_cause,
			timeline: postmortem.timeline.map((entry) => ({ ...entry })),
			actions: postmortem.actions.map((item) => ({ ...item })),
			published: postmortem.published
		};
	}
	return { summary: '', impact: '', root_cause: '', timeline: timelineFromUpdates(updates), actions: [], published: false };
}

/** The draft as sent: trimmed, rows without text dropped, timeline in time order. */
export function cleanDraft(draft: PostmortemPayload): PostmortemPayload {
	return {
		summary: draft.summary.trim(),
		impact: draft.impact.trim(),
		root_cause: draft.root_cause.trim(),
		timeline: draft.timeline
			.map((entry) => ({ at: entry.at, text: entry.text.trim() }))
			.filter((entry) => entry.text !== '' && entry.at !== '')
			.sort((a, b) => Date.parse(a.at) - Date.parse(b.at)),
		actions: draft.actions.map((item) => ({ text: item.text.trim(), done: item.done })).filter((item) => item.text !== ''),
		published: draft.published
	};
}

export type PublishBlock = 'not_resolved' | 'no_summary' | null;

/** Why the draft cannot be published yet, mirroring the server. */
export function publishBlock(status: IncidentStatus, draft: PostmortemPayload): PublishBlock {
	if (status !== 'resolved') return 'not_resolved';
	if (draft.summary.trim() === '') return 'no_summary';
	return null;
}

/** Plain text split into paragraphs on blank lines: what the public page renders. */
export function paragraphs(text: string): string[] {
	return text
		.split(/\n\s*\n/)
		.map((part) => part.trim())
		.filter((part) => part !== '');
}
