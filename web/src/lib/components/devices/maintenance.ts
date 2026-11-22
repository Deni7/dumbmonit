/**
 * Which maintenance windows (silences) cover a whole monitor: the ones aimed
 * at it, the ones on every monitor, and the ones matching its tags. A per-alert
 * snooze (a silence that also carries label matchers beyond tags) quiets one
 * alert only and is left out. Pure, so `node --test` can load it.
 */
import type { Silence, Target } from '#lib/api/index.js';

const TAG_PREFIX = 'tag_';

/** Does this silence quiet the whole monitor, not just one of its alerts? */
export function coversMonitor(silence: Silence, target: Pick<Target, 'id' | 'tags'>): boolean {
	if (silence.target_id !== null && silence.target_id !== target.id) return false;
	const matchers = Object.entries(silence.matchers ?? {});
	if (silence.target_id !== null) return matchers.length === 0;
	// Every monitor, or the ones whose tags match: any other label narrows it to a single alert.
	return matchers.every(([key, value]) => key.startsWith(TAG_PREFIX) && target.tags[key.slice(TAG_PREFIX.length)] === value);
}

const when = (value: string | null | undefined) => (value ? Date.parse(/(Z|[+-]\d\d:?\d\d)$/.test(value) ? value : `${value}Z`) : NaN);

/** Active windows first (the nearest end first), then the upcoming ones by start. Disabled and finished ones are dropped. */
export function windowsFor(silences: readonly Silence[], target: Pick<Target, 'id' | 'tags'>): Silence[] {
	const rank = (s: Silence) => (s.active_now ? 0 : 1);
	const at = (s: Silence) => {
		const t = s.active_now ? when(s.active_until) : when(s.next_start_at);
		return Number.isNaN(t) ? Infinity : t;
	};
	return silences
		.filter((s) => s.enabled && (s.active_now || s.next_start_at) && coversMonitor(s, target))
		.sort((a, b) => rank(a) - rank(b) || at(a) - at(b));
}
