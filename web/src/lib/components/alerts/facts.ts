/**
 * What an alert row carries so nobody has to open the monitor: pure helpers
 * (no DOM, no network, no messages) for the details unfolded under a row and
 * for the "updated … ago" indicator.
 */
import type { AlertHistoryEntry, RuleOperator } from '#lib/api/index.js';

/** The comparison as it reads: `>=` becomes "≥". */
export function operatorSymbol(operator: RuleOperator): string {
	if (operator === '>=') return '≥';
	if (operator === '<=') return '≤';
	return operator;
}

/**
 * The latest recorded phase transitions of these alerts (the row's own and the
 * folded siblings), newest first. History already travels with the page, so
 * this costs no request. Entries the user hid are left out.
 */
export function recentTransitions(
	history: readonly AlertHistoryEntry[],
	fingerprints: readonly string[],
	limit = 4
): AlertHistoryEntry[] {
	if (fingerprints.length === 0) return [];
	const wanted = new Set(fingerprints);
	return history
		.filter((entry) => wanted.has(entry.fingerprint) && !entry.dismissed)
		.sort((a, b) => Date.parse(b.at) - Date.parse(a.at))
		.slice(0, limit);
}

/** Whole seconds between a moment and now; `null` when the moment is unknown. */
export function secondsSince(moment: Date | null, now: number): number | null {
	if (!moment) return null;
	return Math.max(0, Math.round((now - moment.getTime()) / 1000));
}

/** Past this age (several missed polls) the list is called stale. */
export const STALE_AFTER_SECS = 90;

/** True when the last successful refresh is old enough to doubt what is shown. */
export function isStale(updatedAt: number | null, now: number): boolean {
	return updatedAt !== null && now - updatedAt > STALE_AFTER_SECS * 1000;
}
