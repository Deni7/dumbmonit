/**
 * Status and event → fill, tone and ink for the overview, in the console's
 * ladder: `signal` up, `advisory` degraded or misconfigured (amber),
 * `warning` down (red), `info` blue, `muted` paused or taken care of. Colour
 * is never alone: a bar carries its state in its tooltip, an event its
 * icon and its sentence. Pure on purpose (types only).
 */
import type { Tone } from '#lib/ui/index.js';
import type { WidgetEvent } from '#lib/api/types/overview.js';

/** The console's states (`enterprise/status.ts`), a superset of the widgets' `WidgetStatus`. */
export type DashStatus = 'up' | 'degraded' | 'down' | 'paused' | 'config' | 'waiting';

/** Fill of a bar for a monitor in this state (token based). */
export const STATUS_FILL: Record<DashStatus, string> = {
	up: 'var(--color-river-up)',
	degraded: 'var(--color-advisory)',
	down: 'var(--color-warning)',
	paused: 'var(--color-line-strong)',
	config: 'var(--color-advisory)',
	waiting: 'var(--color-ghost)'
};

/** Text ink of a tone (an icon, a figure that backs a plate up). */
export const TONE_INK: Record<Tone, string> = {
	signal: 'text-signal-ink',
	info: 'text-info-ink',
	advisory: 'text-advisory-ink',
	warning: 'text-warning-ink',
	ghost: 'text-ink-2',
	muted: 'text-ink-3'
};

export const EVENT_TONE: Record<WidgetEvent['kind'], Tone> = {
	crit: 'warning',
	warn: 'advisory',
	ok: 'signal',
	info: 'info',
	maint: 'info',
	ack: 'muted'
};
