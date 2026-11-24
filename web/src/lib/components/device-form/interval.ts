/**
 * The free check interval of the monitor form: a number and a unit beside the
 * presets. The bounds mirror `MIN_INTERVAL_SECS` and `MAX_INTERVAL_SECS` in
 * `crates/server/src/api/targets.rs` (10 s to 7 days).
 */
export const MIN_INTERVAL_SECS = 10;
export const MAX_INTERVAL_SECS = 7 * 24 * 3600;

export type IntervalUnit = 's' | 'm' | 'h' | 'd';
export const INTERVAL_UNITS: readonly IntervalUnit[] = ['s', 'm', 'h', 'd'];
const UNIT_SECS: Record<IntervalUnit, number> = { s: 1, m: 60, h: 3600, d: 86400 };

/** The largest unit that divides the interval evenly: 7200 s is "2 h", 90 s stays "90 s". */
export function splitInterval(secs: number): { value: number; unit: IntervalUnit } {
	for (const unit of ['d', 'h', 'm'] as const) {
		if (secs >= UNIT_SECS[unit] && secs % UNIT_SECS[unit] === 0) {
			return { value: secs / UNIT_SECS[unit], unit };
		}
	}
	return { value: secs, unit: 's' };
}

/** Seconds for a typed value and unit; `null` when it is not a whole number above zero. */
export function intervalSeconds(value: string | number, unit: IntervalUnit): number | null {
	const text = String(value).trim();
	if (!/^\d+$/.test(text)) return null;
	const secs = Number(text) * UNIT_SECS[unit];
	return Number.isSafeInteger(secs) && secs > 0 ? secs : null;
}

export type IntervalProblem = 'invalid' | 'min' | 'max';

/** What is wrong with an interval in seconds (`null`: a value that is not a number), if anything. */
export function intervalProblem(secs: number | null): IntervalProblem | null {
	if (secs === null) return 'invalid';
	if (secs < MIN_INTERVAL_SECS) return 'min';
	if (secs > MAX_INTERVAL_SECS) return 'max';
	return null;
}
