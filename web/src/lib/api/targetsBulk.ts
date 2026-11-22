/**
 * Bulk edit of monitors: one request for a selection. Mirrors
 * `crates/server/src/api/targets_bulk.rs`. Each monitor gets its own verdict;
 * those that pass are written together.
 */
import { request } from './client';
import type { TargetId } from './types';

/** Notification channels of each monitor: replace the list, add to it or remove from it. */
export type ChannelsPatch = { set: number[] } | { add: number[] } | { remove: number[] };

/** What a bulk edit changes; a field left out stays as it is. */
export interface TargetBulkPatch {
	interval_secs?: number;
	channels?: ChannelsPatch;
	/** Free tags only: an option of the monitor's type is refused when added, left alone when removed. */
	tags?: { add?: Record<string, string>; remove?: string[] };
	/** Destination folder; empty: no folder. */
	group_name?: string;
}

export interface TargetBulkItem {
	id: TargetId;
	ok: boolean;
	/** Why this monitor was left as it was. */
	error?: string;
}

export interface TargetBulkReport {
	updated: number;
	failed: number;
	/** In the order of the request. */
	results: TargetBulkItem[];
}

export function bulkUpdateTargets(ids: TargetId[], patch: TargetBulkPatch): Promise<TargetBulkReport> {
	return request<TargetBulkReport>('/targets/bulk', { method: 'PATCH', body: { ids, patch } });
}
