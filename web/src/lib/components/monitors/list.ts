/**
 * Pure helpers of the monitors list (`MonitorList.svelte`): which devices the
 * search, the kind, the site and the tags leave, how they split into the state
 * segments, the optimistic overrides of a move or a reorder, and the payload
 * a save echoes. The URL state, sorting and bulk runs live in
 * `devices/listState.ts`.
 */
import type { Target, TargetId, TargetPayload } from '#lib/api/index.js';
import type { TargetState } from '#lib/format.js';
// Explicit extensions: also loaded straight by Node (`npm test`).
import { needsAttention, type RackRow } from '../devices/rack.ts';
import { freeTags, hasAllTags, matchesSite, type Segment } from '../devices/listState.ts';

export interface Narrowing {
	search: string;
	kind: string;
	site: string;
	tags: string[];
}

/** Devices matching the search box, the kind select, the site and the tags, before the segment. */
export function narrowTargets(
	targets: Target[],
	{ search, kind, site, tags }: Narrowing,
	kindLabels: ReadonlyMap<string, string>,
	optionKeysOf: (kind: string) => ReadonlySet<string>
): Target[] {
	const term = search.trim().toLowerCase();
	return targets.filter((target) => {
		if (kind && target.kind !== kind) return false;
		if (!matchesSite(target.location_id, site)) return false;
		if (!hasAllTags(target.tags, tags, optionKeysOf(target.kind))) return false;
		if (!term) return true;
		const label = kindLabels.get(target.kind) ?? '';
		return (
			target.name.toLowerCase().includes(term) ||
			target.address.toLowerCase().includes(term) ||
			target.kind.toLowerCase().includes(term) ||
			label.toLowerCase().includes(term) ||
			// The free tags (`key=value`) only: an option of the type may hold a secret.
			freeTags(target.tags, optionKeysOf(target.kind)).some((tag) => tag.toLowerCase().includes(term))
		);
	});
}

export type SegmentCounts = Record<Segment, number>;

/** How many devices each state segment holds. */
export function countSegments(targets: Target[], stateOf: (target: Target) => TargetState): SegmentCounts {
	const c = { all: targets.length, attention: 0, reporting: 0, disabled: 0 };
	for (const target of targets) {
		const state = stateOf(target);
		if (needsAttention(state)) c.attention += 1;
		else if (state === 'online') c.reporting += 1;
		else if (state === 'disabled') c.disabled += 1;
	}
	return c;
}

/** Whether a device in this state belongs to the segment. */
export function inSegment(state: TargetState, segment: Segment): boolean {
	return (
		segment === 'all' ||
		(segment === 'attention' && needsAttention(state)) ||
		(segment === 'reporting' && state === 'online') ||
		(segment === 'disabled' && state === 'disabled')
	);
}

/** Ids of the devices the segment keeps. */
export function visibleIds(
	targets: Target[],
	segment: Segment,
	stateOf: (target: Target) => TargetState
): Set<TargetId> {
	const ids = new Set<TargetId>();
	for (const target of targets) if (inSegment(stateOf(target), segment)) ids.add(target.id);
	return ids;
}

/** Kinds offered by the filter: only those a monitor uses (plus the current one), by label. */
export function kindChoices(
	targets: Target[],
	current: string,
	kindLabels: ReadonlyMap<string, string>
): [string, string][] {
	const seen = new Map<string, string>();
	for (const target of targets) seen.set(target.kind, kindLabels.get(target.kind) ?? target.kind);
	if (current && !seen.has(current)) seen.set(current, kindLabels.get(current) ?? current);
	return [...seen].sort((a, b) => a[1].localeCompare(b[1], 'en'));
}

/** What "Filter" hides that is not at its default: the count on the button. */
export function countActiveFilters(f: {
	segment: Segment;
	kind: string;
	site: string;
	tags: string[];
	sorted: boolean;
}): number {
	return (f.segment !== 'all' ? 1 : 0) + (f.kind ? 1 : 0) + (f.site ? 1 : 0) + f.tags.length + (f.sorted ? 1 : 0);
}

/** Rows that match the filters, without the parents shown as context. */
export function matchCount(rows: RackRow[]): number {
	return rows.reduce((n, row) => n + (row.context ? 0 : 1), 0);
}

/** Rows needing attention, said on a folder header only while it is folded away. */
export function troubleCount(rows: RackRow[]): number {
	return rows.reduce((n, row) => n + (!row.context && needsAttention(row.state) ? 1 : 0), 0);
}

/** The store's devices with the optimistic position and folder of a pending move applied. */
export function withOverrides(
	targets: Target[],
	positions: ReadonlyMap<TargetId, number>,
	groups: ReadonlyMap<TargetId, string>
): Target[] {
	if (positions.size === 0 && groups.size === 0) return targets;
	return targets.map((t) => {
		const pos = positions.get(t.id);
		const grp = groups.get(t.id);
		if (pos === undefined && grp === undefined) return t;
		return { ...t, position: pos ?? t.position, group_name: grp ?? t.group_name };
	});
}

/** Rolls overrides back to what they were before a failed save (`undefined`: there was none). */
export function restoreOverrides<V>(
	current: ReadonlyMap<TargetId, V>,
	previous: ReadonlyMap<TargetId, V | undefined>
): Map<TargetId, V> {
	const rolled = new Map(current);
	for (const [id, value] of previous) {
		if (value === undefined) rolled.delete(id);
		else rolled.set(id, value);
	}
	return rolled;
}

/** A copy of the set with `value` added, or removed when it was there. */
export function toggled<T>(set: ReadonlySet<T>, value: T): Set<T> {
	const next = new Set(set);
	if (next.has(value)) next.delete(value);
	else next.add(value);
	return next;
}

/** Builds the payload a save needs to echo every field the server does not keep by itself. */
export function targetToPayload(target: Target, patch: Partial<TargetPayload> = {}): TargetPayload {
	return {
		name: target.name,
		address: target.address,
		kind: target.kind,
		parent_id: target.parent_id,
		via_agent: target.via_agent,
		interval_secs: target.interval_secs,
		enabled: target.enabled,
		tags: target.tags,
		group_name: target.group_name,
		...patch
	};
}
