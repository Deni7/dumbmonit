/**
 * The slots of a monitor's bar ("Last 24 h"), from the worst health measured in each.
 *
 * A probe (HTTP, TCP…) writes `dumbmonit_probe_success` 0 or 1 at every check:
 * an empty slot means nothing was measured. A device writes nothing but
 * `dumbmonit_up = 1`, and only when it answers: when it does not, the series
 * stops, and that silence is what the "Monitor unreachable" rule reads. So for
 * a device checked often enough to write in every slot, an empty slot after its
 * first measurement is an hour it did not answer: red, not grey.
 *
 * Pure, without imports, so that the tests can load it as is.
 */

/** One slot of a monitor's bar: every check passed, some failed, all failed, nothing measured. */
export type RiverCell = 'up' | 'warn' | 'down' | 'none';

export function cellOf(value: number | undefined): RiverCell {
	if (value === undefined) return 'none';
	if (value >= 0.999) return 'up';
	if (value <= 0.001) return 'down';
	return 'warn';
}

/**
 * Whether an empty slot of `stepSecs` means "did not answer" for this monitor:
 * an enabled device (no probe verdict of its own) checked at least twice per
 * slot, so that one check late or slow never empties a slot it answered in.
 */
export function silenceMeansDown(
	monitor: { enabled: boolean; interval_secs: number },
	hasProbeVerdict: boolean,
	stepSecs: number
): boolean {
	return monitor.enabled && !hasProbeVerdict && monitor.interval_secs > 0 && monitor.interval_secs * 2 <= stepSecs;
}

/**
 * The cells `1..slots` of one monitor from its worst value per slot. With
 * `silentIsDown`, a slot without a measurement after the first measured one
 * reads `down`; before it, the monitor may not have existed yet: `none`.
 */
export function riverCells(bySlot: ReadonlyMap<number, number>, slots: number, silentIsDown: boolean): RiverCell[] {
	const cells: RiverCell[] = [];
	let seen = false;
	for (let i = 1; i <= slots; i += 1) {
		const value = bySlot.get(i);
		if (value !== undefined) seen = true;
		cells.push(value === undefined && silentIsDown && seen ? 'down' : cellOf(value));
	}
	return cells;
}
