/**
 * Status → tone, fill and glyph for the dashboard widgets, in the console's
 * ladder: `signal` up, `advisory` degraded or misconfigured (amber),
 * `warning` down (red), `info` blue, `muted` paused or taken care of. Colour
 * is never alone: every plate carries a word and every mark a glyph
 * (`Glyph.svelte`). Pure on purpose (types only).
 */
import type { Tone } from '#lib/ui/index.js';
import type { WidgetEvent } from '#lib/api/types/overview.js';

/** The console's states (`enterprise/status.ts`), a superset of the widgets' `WidgetStatus`. */
export type DashStatus = 'up' | 'degraded' | 'down' | 'paused' | 'config' | 'waiting';

export type GlyphName = 'ok' | 'crit' | 'warn' | 'info' | 'paused' | 'maint' | 'run' | 'ack' | 'up' | 'down' | 'wait';

export const STATUS_TONE: Record<DashStatus, Tone> = {
	up: 'signal',
	degraded: 'advisory',
	down: 'warning',
	paused: 'muted',
	config: 'advisory',
	waiting: 'ghost'
};

/** Fill of a cell or bar for a monitor in this state (token based). */
export const STATUS_FILL: Record<DashStatus, string> = {
	up: 'var(--color-river-up)',
	degraded: 'var(--color-advisory)',
	down: 'var(--color-warning)',
	paused: 'var(--color-line-strong)',
	config: 'var(--color-advisory)',
	waiting: 'var(--color-ghost)'
};

export const STATUS_GLYPH: Record<DashStatus, GlyphName> = {
	up: 'ok',
	degraded: 'warn',
	down: 'crit',
	paused: 'paused',
	config: 'warn',
	waiting: 'wait'
};

/** Alert severity → fill, for the stacked bars and their legend. */
export const SEVERITY_FILL: Record<'critical' | 'warning' | 'info', string> = {
	critical: 'var(--color-warning)',
	warning: 'var(--color-advisory)',
	info: 'var(--color-info)'
};

/** The soft square behind an event's glyph, and its ink. */
export const TONE_SOFT: Record<Tone, string> = {
	signal: 'bg-signal-soft text-signal-ink',
	info: 'bg-info-soft text-info-ink',
	advisory: 'bg-advisory-soft text-advisory-ink',
	warning: 'bg-warning-soft text-warning-ink',
	ghost: 'bg-ghost text-ink-2',
	muted: 'bg-surface-2 text-ink-3'
};

/** Text ink of a tone (for a figure that backs a plate up). */
export const TONE_INK: Record<Tone, string> = {
	signal: 'text-signal-ink',
	info: 'text-info-ink',
	advisory: 'text-advisory-ink',
	warning: 'text-warning-ink',
	ghost: 'text-ink-2',
	muted: 'text-ink-3'
};

type EventKind = WidgetEvent['kind'];

export const EVENT_TONE: Record<EventKind, Tone> = {
	crit: 'warning',
	warn: 'advisory',
	ok: 'signal',
	info: 'info',
	maint: 'info',
	ack: 'muted'
};

export const EVENT_GLYPH: Record<EventKind, GlyphName> = {
	crit: 'crit',
	warn: 'warn',
	ok: 'ok',
	info: 'info',
	maint: 'maint',
	ack: 'ack'
};

/** Alert events (fired, resolved, acknowledged) against changes (installs, edits, maintenance). */
export function eventGroup(kind: EventKind): 'alerts' | 'changes' {
	return kind === 'info' || kind === 'maint' ? 'changes' : 'alerts';
}
