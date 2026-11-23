/**
 * Change markers as chart events: the shape `Chart.svelte` draws (a solid line,
 * a flag, a shaded window), built from what the API returns.
 */
import type { Annotation } from '#lib/api/index.js';
import type { ChartEvent } from '#lib/components/Chart.svelte';
import { parseServerDate } from '#lib/format.js';

/** Seconds since the epoch of a server time (UTC without suffix); null when unreadable. */
export function serverSeconds(value: string | null | undefined): number | null {
	const date = parseServerDate(value);
	return date ? Math.floor(date.getTime() / 1000) : null;
}

/** A marker as a chart event; `null` when its time cannot be read. */
export function toChartEvent(annotation: Annotation): ChartEvent | null {
	const at = serverSeconds(annotation.at);
	if (at === null) return null;
	const until = serverSeconds(annotation.until);
	return {
		at,
		label: annotation.title,
		tone: 'signal',
		kind: 'change',
		until: until !== null && until > at ? until : undefined,
		detail: annotation.text || undefined,
		href: /^https?:\/\//.test(annotation.link) ? annotation.link : undefined
	};
}

export function toChartEvents(annotations: Annotation[]): ChartEvent[] {
	return annotations.flatMap((annotation) => toChartEvent(annotation) ?? []);
}
