/**
 * "Last seen" in its shortest form: the largest whole unit, rounded down
 * (59 s, 2 min, 3 h, 5 d). Pure, so it can be tested without the i18n layer;
 * `formatAge` in `format.ts` turns the parts into the localised short unit.
 */
export type AgeUnit = 's' | 'min' | 'h' | 'd';

export function ageParts(seconds: number): { n: number; unit: AgeUnit } {
	const s = Math.max(0, Math.floor(seconds));
	if (s < 60) return { n: s, unit: 's' };
	const minutes = Math.floor(s / 60);
	if (minutes < 60) return { n: minutes, unit: 'min' };
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return { n: hours, unit: 'h' };
	return { n: Math.floor(hours / 24), unit: 'd' };
}
