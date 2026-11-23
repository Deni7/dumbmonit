/**
 * How a rule's threshold is read and typed, decided from its unit alone — no
 * rule is special-cased. Mirrors `crates/server/src/alerting/units.rs`:
 *
 * - `s`, `min`, `h`, `d`: the threshold is a duration stored in that unit; the
 *   editor shows it, and lets it be typed, in minutes, hours or days;
 * - `bool`: the rule reads an on/off signal, there is nothing to set;
 * - anything else (`%`, `°C`, `GiB`…) is a plain suffix.
 *
 * Kept free of Svelte and of the message catalogue so it is testable with node.
 */

export type DurationUnit = 's' | 'm' | 'h' | 'd';

/** Seconds in each unit a duration can be typed in. */
export const DURATION_SECS: Record<DurationUnit, number> = { s: 1, m: 60, h: 3600, d: 86400 };

/** The rule units that are durations, and how many seconds one of them holds. */
const DURATION_BASES: Record<string, number> = { s: 1, min: 60, h: 3600, d: 86400 };

/** Unit of an on/off rule (the series is 0 or 1). */
export const FLAG_UNIT = 'bool';

export type ThresholdShape =
	| { kind: 'flag' }
	/** `base`: seconds per stored unit; `choices`: the units the editor offers. */
	| { kind: 'duration'; base: number; choices: DurationUnit[] }
	| { kind: 'plain'; unit: string };

const ALL_UNITS: DurationUnit[] = ['s', 'm', 'h', 'd'];

/** Every unit, for a "how long" field stored in seconds (the hold duration). */
export const HOLD_UNITS: DurationUnit[] = ALL_UNITS;

export function thresholdShape(unit: string | null | undefined): ThresholdShape {
	const u = unit?.trim() ?? '';
	if (u === FLAG_UNIT) return { kind: 'flag' };
	const base = Object.prototype.hasOwnProperty.call(DURATION_BASES, u) ? DURATION_BASES[u] : undefined;
	if (base !== undefined) {
		// Never finer than the stored unit, and never finer than hours for a rule kept in hours or days.
		const floor = Math.min(base, DURATION_SECS.h);
		return { kind: 'duration', base, choices: ALL_UNITS.filter((c) => DURATION_SECS[c] >= floor) };
	}
	return { kind: 'plain', unit: u };
}

export function isFlagUnit(unit: string | null | undefined): boolean {
	return thresholdShape(unit).kind === 'flag';
}

/** Seconds per stored unit when the rule's unit is a duration, else null. */
export function durationBase(unit: string | null | undefined): number | null {
	const shape = thresholdShape(unit);
	return shape.kind === 'duration' ? shape.base : null;
}

/** Drops floating-point noise (1.9999999 → 2) without hiding a real fraction. */
function tidy(value: number): number {
	return Math.round(value * 1e6) / 1e6;
}

/**
 * The stored value as a number and the unit that reads best: the largest unit
 * it is a whole number of (172800 s is "2 days", 129600 s "36 hours", 90 s stays
 * "90 seconds"). Zero reads in the largest unit offered: "0 days" left on a
 * certificate says more than "0 seconds".
 */
export function splitDuration(
	stored: number,
	base: number,
	choices: DurationUnit[] = ALL_UNITS
): { value: number; unit: DurationUnit } {
	const ordered = [...choices].sort((a, b) => DURATION_SECS[b] - DURATION_SECS[a]);
	const smallest = ordered[ordered.length - 1] ?? 's';
	const secs = stored * base;
	if (!Number.isFinite(secs)) return { value: stored, unit: smallest };
	if (secs === 0) return { value: 0, unit: ordered[0] ?? smallest };
	const abs = Math.abs(secs);
	for (const unit of ordered) {
		const amount = abs / DURATION_SECS[unit];
		if (amount >= 1 && Math.abs(amount - Math.round(amount)) < 1e-9) {
			return { value: tidy(secs / DURATION_SECS[unit]), unit };
		}
	}
	return { value: tidy(secs / DURATION_SECS[smallest]), unit: smallest };
}

/**
 * What the operator typed, back in the rule's stored unit; null when it is
 * empty or not a number. `wholeSeconds` rounds to the second (the hold duration is an
 * integer on the server).
 */
export function toStored(
	text: string | number | null | undefined,
	unit: DurationUnit,
	base: number,
	wholeSeconds = false
): number | null {
	// A bound number field hands back a number, or null once emptied.
	if (text === null || text === undefined) return null;
	const raw = typeof text === 'number' ? text : String(text).trim();
	if (raw === '') return null;
	const value = Number(raw);
	if (!Number.isFinite(value)) return null;
	const secs = value * DURATION_SECS[unit];
	return wholeSeconds ? Math.round(secs) / base : tidy(secs / base);
}

/**
 * The text a duration field starts with, from a stored value. An empty value
 * (no clear threshold) starts on `fallback`, the unit of the field beside it.
 */
export function durationText(
	stored: number | null,
	base: number,
	choices: DurationUnit[] = ALL_UNITS,
	fallback?: DurationUnit
): { text: string; unit: DurationUnit } {
	if (stored === null) {
		const unit = fallback && choices.includes(fallback) ? fallback : splitDuration(0, base, choices).unit;
		return { text: '', unit };
	}
	const split = splitDuration(stored, base, choices);
	return { text: String(split.value), unit: split.unit };
}
