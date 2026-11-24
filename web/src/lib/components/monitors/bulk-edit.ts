/**
 * Pure helpers of the bulk edit dialog (`BulkEditDialog.svelte`): the form
 * turned into the one request `PATCH /api/targets/bulk` takes, the tag typed
 * as `key=value`, the free tag keys of a selection, and the server's verdicts
 * turned into what the list reports.
 */
import type { Target, TargetId } from '#lib/api/index.js';
import type { TargetBulkPatch, TargetBulkReport } from '#lib/api/targetsBulk.js';
// Explicit extension: also loaded straight by Node (`npm test`).
import { freeTags } from '../devices/listState.ts';

export type ChannelMode = 'keep' | 'set' | 'add' | 'remove';

/** "Keep as is" in the interval and folder selects. */
export const KEEP = '';
/** "No folder" in the folder select. */
export const NO_FOLDER = '__dm_no_folder__';
/** "New folder…" in the folder select: the name is typed beside it. */
export const NEW_FOLDER = '__dm_new_folder__';

export interface BulkEditForm {
	/** Seconds, or `KEEP`. */
	interval: string;
	channelMode: ChannelMode;
	channels: ReadonlySet<number>;
	/** `key=value` (or a bare key) to add; empty: none. */
	tagToAdd: string;
	tagsToRemove: ReadonlySet<string>;
	/** A folder name, `KEEP`, `NO_FOLDER` or `NEW_FOLDER`. */
	folder: string;
	newFolder: string;
}

/** `room=rack-1` → `{ key: 'room', value: 'rack-1' }`; a bare key has an empty value. */
export function parseTag(raw: string): { key: string; value: string } | null {
	const text = raw.trim();
	if (!text) return null;
	const eq = text.indexOf('=');
	const key = (eq === -1 ? text : text.slice(0, eq)).trim();
	if (!key) return null;
	return { key, value: eq === -1 ? '' : text.slice(eq + 1).trim() };
}

/** The request the form describes; `null` while it changes nothing. */
export function buildBulkPatch(form: BulkEditForm): TargetBulkPatch | null {
	const patch: TargetBulkPatch = {};
	if (form.interval !== KEEP) {
		const secs = Number(form.interval);
		if (Number.isInteger(secs) && secs > 0) patch.interval_secs = secs;
	}
	const ids = [...form.channels].sort((a, b) => a - b);
	if (form.channelMode === 'set') patch.channels = { set: ids };
	else if (form.channelMode === 'add' && ids.length > 0) patch.channels = { add: ids };
	else if (form.channelMode === 'remove' && ids.length > 0) patch.channels = { remove: ids };
	const added = parseTag(form.tagToAdd);
	const removed = [...form.tagsToRemove].filter((key) => key !== added?.key).sort();
	if (added || removed.length > 0) {
		patch.tags = {};
		if (added) patch.tags.add = { [added.key]: added.value };
		if (removed.length > 0) patch.tags.remove = removed;
	}
	if (form.folder === NO_FOLDER) patch.group_name = '';
	else if (form.folder === NEW_FOLDER) {
		const name = form.newFolder.trim();
		if (name) patch.group_name = name;
	} else if (form.folder !== KEEP) patch.group_name = form.folder;
	return Object.keys(patch).length > 0 ? patch : null;
}

/** The free tag keys the selection carries (the options of each type are not tags), sorted. */
export function freeTagKeys(targets: Target[], optionKeysOf: (kind: string) => ReadonlySet<string>): string[] {
	const keys = new Set<string>();
	for (const target of targets) {
		for (const pair of freeTags(target.tags, optionKeysOf(target.kind))) {
			const at = pair.indexOf('=');
			keys.add(at === -1 ? pair : pair.slice(0, at));
		}
	}
	return [...keys].sort((a, b) => a.localeCompare(b, 'en'));
}

/** The monitors the server left as they were, by name, with its reason. */
export function bulkEditFailures(
	report: TargetBulkReport,
	targets: Target[]
): { ids: TargetId[]; failed: { name: string; error: string }[] } {
	const names = new Map(targets.map((t) => [t.id, t.name]));
	const failures = report.results.filter((item) => !item.ok);
	return {
		ids: failures.map((item) => item.id),
		failed: failures.map((item) => ({ name: names.get(item.id) ?? `#${item.id}`, error: item.error ?? '' }))
	};
}
