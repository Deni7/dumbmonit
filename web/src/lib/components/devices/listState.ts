/**
 * The /targets list state that lives in the URL (search, state segment, kind,
 * tags, sort, layout) so a filtered view can be linked and survives a reload, plus
 * the pure helpers behind sorting and the bulk actions. Defaults are never
 * written, so a pristine list keeps a clean `/targets`.
 */
import type { Target, TargetId } from '#lib/api/index.js';
import type { TargetState } from '#lib/format.js';
// Explicit extensions: also loaded straight by Node (`npm test`).
import { RANK, type RackRow } from './rack.ts';
import { asRackView, type RackView } from './view.ts';

export type Segment = 'all' | 'attention' | 'reporting' | 'disabled';
export const SEGMENTS: readonly Segment[] = ['all', 'attention', 'reporting', 'disabled'];

/** `default` is the rack order (folders, state tier, manual position, name). */
export type SortKey = 'default' | 'name' | 'status' | 'kind' | 'checked' | 'availability' | 'created';
export const SORT_KEYS: readonly SortKey[] = [
	'default',
	'name',
	'status',
	'kind',
	'checked',
	'availability',
	'created'
];
export type SortDir = 'asc' | 'desc';

export interface ListQuery {
	search: string;
	segment: Segment;
	kind: string;
	/** `key=value` free tags, all of which a device must carry. */
	tags: string[];
	/** `''`: all sites, `none`: devices without a location, else a location id. */
	site: string;
	sort: SortKey;
	dir: SortDir;
	/** `null`: not in the URL, the viewer's remembered layout applies. */
	view: RackView | null;
}

export function parseListQuery(params: {
	get(name: string): string | null;
	getAll(name: string): string[];
}): ListQuery {
	const segment = params.get('state');
	const sort = params.get('sort');
	const view = params.get('view');
	return {
		search: params.get('q') ?? '',
		segment: (SEGMENTS as readonly (string | null)[]).includes(segment) ? (segment as Segment) : 'all',
		kind: params.get('kind') ?? '',
		tags: params.getAll('tag').filter((t) => t.includes('=')),
		site: params.get('site') ?? '',
		sort: (SORT_KEYS as readonly (string | null)[]).includes(sort) ? (sort as SortKey) : 'default',
		dir: params.get('dir') === 'desc' ? 'desc' : 'asc',
		// `?view=rows` (retired layout) opens compact, the default.
		view: asRackView(view)
	};
}

/** Query string (with the leading `?`, or empty) for a state; defaults are left out. */
export function serializeListQuery(query: ListQuery): string {
	const params = new URLSearchParams();
	if (query.search.trim()) params.set('q', query.search.trim());
	if (query.segment !== 'all') params.set('state', query.segment);
	if (query.kind) params.set('kind', query.kind);
	for (const tag of query.tags) params.append('tag', tag);
	if (query.site) params.set('site', query.site);
	if (query.sort !== 'default') {
		params.set('sort', query.sort);
		if (query.dir === 'desc') params.set('dir', 'desc');
	}
	if (query.view) params.set('view', query.view);
	const text = params.toString();
	return text ? `?${text}` : '';
}

const QUERY_KEY = 'dumbmonit-targets-list';

/**
 * Remembers the filters, sort and layout of the list in this browser (never
 * the search text: a stale search would look like a broken list). A pristine
 * state forgets it. Every access is guarded: private windows throw.
 */
export function saveListQuery(query: ListQuery): void {
	try {
		const text = serializeListQuery({ ...query, search: '' });
		if (text) localStorage.setItem(QUERY_KEY, text);
		else localStorage.removeItem(QUERY_KEY);
	} catch {
		/* Storage unavailable: the list just starts fresh next time. */
	}
}

/** The remembered state, or `null` when there is none (or storage is blocked). */
export function loadListQuery(): ListQuery | null {
	try {
		const text = localStorage.getItem(QUERY_KEY);
		return text ? parseListQuery(new URLSearchParams(text)) : null;
	} catch {
		return null;
	}
}

/** Share of the hours with data in which the monitor was up (0..1), `null` without any data. */
export function availabilityOf(cells: readonly string[] | null | undefined): number | null {
	if (!cells) return null;
	let up = 0;
	let known = 0;
	for (const cell of cells) {
		if (cell === 'none') continue;
		known += 1;
		if (cell === 'up') up += 1;
	}
	return known === 0 ? null : up / known;
}

/**
 * A device's `tags` hold both the options of its type (`keyword`, `port`…) and
 * the free tags the user added; only the latter are tags for filtering.
 * Returned as `key=value`.
 */
export function freeTags(tags: Record<string, string>, optionKeys: ReadonlySet<string>): string[] {
	return Object.entries(tags)
		.filter(([key, value]) => !optionKeys.has(key) && value !== '')
		.map(([key, value]) => `${key}=${value}`);
}

export function hasAllTags(
	tags: Record<string, string>,
	wanted: readonly string[],
	optionKeys: ReadonlySet<string>
): boolean {
	if (wanted.length === 0) return true;
	const own = new Set(freeTags(tags, optionKeys));
	return wanted.every((tag) => own.has(tag));
}

/** Every free tag in use with its device count, most used first. */
export function tagCounts(
	devices: readonly { kind: string; tags: Record<string, string> }[],
	optionKeysOf: (kind: string) => ReadonlySet<string>
): [string, number][] {
	const counts = new Map<string, number>();
	for (const device of devices) {
		for (const tag of freeTags(device.tags, optionKeysOf(device.kind))) {
			counts.set(tag, (counts.get(tag) ?? 0) + 1);
		}
	}
	return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], 'en'));
}

/**
 * The tags a picker offers: those in use plus any already chosen (a stale
 * `?tag=` stays visible so it can be cleared), narrowed by what was typed
 * (case-insensitive, any part of the tag). Each carries its device count.
 */
export function tagChoices(
	counts: readonly [string, number][],
	chosen: readonly string[],
	query: string
): [string, number][] {
	const known = new Map(counts);
	const all: [string, number][] = [...new Set([...chosen, ...counts.map(([tag]) => tag)])].map(
		(tag) => [tag, known.get(tag) ?? 0]
	);
	const term = query.trim().toLowerCase();
	return term ? all.filter(([tag]) => tag.toLowerCase().includes(term)) : all;
}

/** Whether a device passes the site filter (`''` all, `none` unplaced, else a location id). */
export function matchesSite(locationId: number | null | undefined, site: string): boolean {
	if (!site) return true;
	if (site === 'none') return locationId == null;
	return locationId != null && String(locationId) === site;
}

const byName = (a: Target, b: Target) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' });

/**
 * Flat, sorted rows for any explicit sort (no folders, no tree: a sorted
 * list is a flat one). `kindLabel` gives the label the kind column shows.
 * A missing last check, availability or creation date always sorts last,
 * whatever the direction. Availability ascending puts the worst first;
 * creation ascending puts the oldest first.
 */
export function sortTargets(
	targets: Target[],
	stateOf: (target: Target) => TargetState,
	kindLabel: (kind: string) => string,
	sort: Exclude<SortKey, 'default'>,
	dir: SortDir,
	/** Availability over the last day (0..1), `null` without data; only the `availability` sort reads it. */
	availability: (target: Target) => number | null = () => null
): RackRow[] {
	const sign = dir === 'desc' ? -1 : 1;
	const states = new Map(targets.map((t) => [t.id, stateOf(t)]));
	const when = (t: Target) => (t.last_probe_at ? Date.parse(t.last_probe_at) : NaN);
	/** Orders two optional numbers, the missing ones last whatever the direction. */
	const byOptional = (a: number | null, b: number | null, a2: Target, b2: Target): number => {
		if (a === null || b === null) {
			if (a === null && b === null) return byName(a2, b2);
			return a === null ? 1 : -1;
		}
		return a === b ? byName(a2, b2) : sign * (a - b);
	};
	const created = (t: Target) => {
		const at = t.created_at ? Date.parse(`${t.created_at}${/(Z|[+-]\d\d:?\d\d)$/.test(t.created_at) ? '' : 'Z'}`) : NaN;
		return Number.isNaN(at) ? null : at;
	};
	const compare = (a: Target, b: Target): number => {
		switch (sort) {
			case 'name':
				return sign * byName(a, b);
			case 'kind': {
				const c = kindLabel(a.kind).localeCompare(kindLabel(b.kind), 'en', { sensitivity: 'base' });
				return c !== 0 ? sign * c : byName(a, b);
			}
			case 'status': {
				const c = RANK[states.get(a.id)!] - RANK[states.get(b.id)!];
				return c !== 0 ? sign * c : byName(a, b);
			}
			case 'checked': {
				const wa = when(a);
				const wb = when(b);
				if (Number.isNaN(wa) || Number.isNaN(wb)) {
					if (Number.isNaN(wa) && Number.isNaN(wb)) return byName(a, b);
					return Number.isNaN(wa) ? 1 : -1;
				}
				// Ascending: most recently checked first.
				return wa === wb ? byName(a, b) : sign * (wb - wa);
			}
			case 'availability':
				return byOptional(availability(a), availability(b), a, b);
			case 'created':
				return byOptional(created(a), created(b), a, b);
		}
	};
	return targets
		.slice()
		.sort(compare)
		.map((target) => ({ target, state: states.get(target.id)!, depth: 0, shadowed: false }));
}

/** Splits ids into active and disabled, by the displayed state. */
export function splitDisabled(
	targets: Target[],
	stateOf: (target: Target) => TargetState
): { active: Set<TargetId>; disabled: Set<TargetId> } {
	const active = new Set<TargetId>();
	const disabled = new Set<TargetId>();
	for (const target of targets) (stateOf(target) === 'disabled' ? disabled : active).add(target.id);
	return { active, disabled };
}

export interface BulkOutcome {
	ok: TargetId[];
	failed: { id: TargetId; name: string; error: unknown }[];
}

/** Ids of the monitors whose CVE check is not yet in the wanted state (an absent flag means on). */
export function cveScanChanges(targets: Target[], enabled: boolean): TargetId[] {
	return targets.filter((t) => (t.cve_scan ?? true) !== enabled).map((t) => t.id);
}

/** Runs `action` over `targets` one after the other, collecting failures instead of stopping at the first. */
export async function runBulk(
	targets: Target[],
	action: (target: Target) => Promise<unknown>
): Promise<BulkOutcome> {
	const outcome: BulkOutcome = { ok: [], failed: [] };
	for (const target of targets) {
		try {
			await action(target);
			outcome.ok.push(target.id);
		} catch (error) {
			outcome.failed.push({ id: target.id, name: target.name, error });
		}
	}
	return outcome;
}
