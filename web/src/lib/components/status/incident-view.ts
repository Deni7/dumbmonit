import type { PublicIncident } from '#lib/api/types.js';

/**
 * Small pure rules of the public page's incident cards and service list:
 * anchors, resolution time, the service filter. Kept apart from the
 * components so they can be tested on their own.
 */

/** Services above which the public page offers a search and a "problems only" switch. */
export const FILTER_THRESHOLD = 8;

/** Anchor id of an incident card; the permalink is `#incident-<anchor>` (start and title, never the internal id). */
export function incidentAnchor(anchor: string): string {
	return `incident-${anchor}`;
}

/** The incident anchor a `location.hash` points at, or `null`. */
export function incidentFromHash(hash: string): string | null {
	const match = /^#incident-([a-z0-9-]+)$/.exec(hash);
	return match ? match[1] : null;
}

function parse(value: string): number {
	const text = value.replace(' ', 'T');
	return Date.parse(/[zZ]|[+-]\d{2}:?\d{2}$/.test(text) ? text : `${text}Z`);
}

/** Seconds from the start to the end of an announcement; `null` while open or on a bad date. */
export function durationSeconds(startsAt: string, endsAt: string | null | undefined): number | null {
	if (!endsAt) return null;
	const seconds = (parse(endsAt) - parse(startsAt)) / 1000;
	return Number.isFinite(seconds) && seconds >= 0 ? seconds : null;
}

interface FilterableItem {
	label: string;
	state: string;
}
interface FilterableGroup<T extends FilterableItem> {
	name: string;
	items: T[];
}

/** A service "with a problem" is down or degraded. */
export function hasProblem(item: FilterableItem): boolean {
	return item.state === 'down' || item.state === 'degraded';
}

/** The groups left by a text search and the "problems only" switch; empty groups are dropped. */
export function filterGroups<T extends FilterableItem, G extends FilterableGroup<T>>(groups: G[], query: string, onlyProblems: boolean): G[] {
	const needle = query.trim().toLocaleLowerCase();
	if (!needle && !onlyProblems) return groups;
	return groups
		.map((group) => ({
			...group,
			items: group.items.filter((item) => (!needle || item.label.toLocaleLowerCase().includes(needle) || group.name.toLocaleLowerCase().includes(needle)) && (!onlyProblems || hasProblem(item)))
		}))
		.filter((group) => group.items.length > 0);
}

/** Inserts a canned text into a message: it fills an empty field, and follows what is already written. */
export function insertTemplate(current: string, text: string): string {
	return current.trim() === '' ? text : `${current.trimEnd()}\n\n${text}`;
}

interface DraftInput {
	id?: number;
	kind: 'incident' | 'maintenance';
	title: string;
	severity: 'minor' | 'major';
	status: string;
	startsAt: string;
	endsAt: string | null;
	/** What the thread already says, oldest first. */
	updates: { status: string; body: string; created_at: string }[];
	/** The message being written; blank: nothing added. */
	message: string;
	/** The moment of the message (ISO). */
	now: string;
}

/** An announcement as the public page would draw it with the message posted: the admin's preview. */
export function draftIncident(input: DraftInput): PublicIncident {
	const message = input.message.trim();
	const updates = message ? [...input.updates, { status: input.status, body: message, created_at: input.now }] : input.updates;
	const closing = input.status === 'resolved' || input.status === 'completed';
	return {
		anchor: 'draft',
		title: input.title.trim() || '…',
		kind: input.kind,
		status: input.status,
		severity: input.severity,
		starts_at: input.startsAt,
		ends_at: closing ? (input.endsAt ?? input.now) : input.endsAt,
		updates
	} as PublicIncident;
}
