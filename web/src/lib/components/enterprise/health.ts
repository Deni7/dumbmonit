/**
 * The overview's fleet health, as plain functions: which monitors count as a
 * problem, how the tile grid shrinks with the fleet, and what the 24 hour bar
 * of a monitor says (how long it has been down, how much of the day it was up).
 * No imports but types, so it runs under `node --test`.
 */
import type { RiverCell } from './fleet.js';
import type { ConsoleStatus } from './status.js';

/** Statuses that belong in "what is wrong": the others are fine, waiting or on purpose paused. */
const PROBLEM: ReadonlySet<ConsoleStatus> = new Set(['down', 'degraded', 'config']);

export function isProblem(status: ConsoleStatus): boolean {
	return PROBLEM.has(status);
}

/** Most tiles drawn; the rest is counted ("+N") and lives in the Monitors list. */
export const TILE_LIMIT = 200;

/** Tile side in px: roomy (and a fair tap target) for a small fleet, small for a large one. */
export function tileSize(count: number): number {
	if (count <= 36) return 40;
	if (count <= 100) return 24;
	return 16;
}

export function splitTiles<T>(rows: readonly T[], limit = TILE_LIMIT): { shown: T[]; hidden: number } {
	return { shown: rows.slice(0, limit), hidden: Math.max(0, rows.length - limit) };
}

/**
 * How long the monitor has been down at the end of the day: the run of `down`
 * slots up to now, in seconds. A last slot with nothing measured yet is skipped;
 * 0 when the last measured slot is not down.
 */
export function outageSeconds(cells: readonly RiverCell[], slotSeconds: number): number {
	let end = cells.length;
	while (end > 0 && cells[end - 1] === 'none') end -= 1;
	let run = 0;
	while (end - run > 0 && cells[end - run - 1] === 'down') run += 1;
	return run * slotSeconds;
}

/** Share of measured slots that were fully up, in percent; `null` when nothing was measured. */
export function dayAvailability(cells: readonly RiverCell[]): number | null {
	let known = 0;
	let up = 0;
	for (const cell of cells) {
		if (cell === 'none') continue;
		known += 1;
		if (cell === 'up') up += 1;
	}
	return known === 0 ? null : (up / known) * 100;
}
