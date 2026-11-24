/**
 * What the wall's banner announces: the incident or maintenance published on
 * the status pages that is still open, or a maintenance window covering the
 * present instant. Pure (types only as imports): also loaded by the tests.
 */
import type { Incident, Silence } from '#lib/api/types.js';

export interface WallNotice {
	key: string;
	kind: 'incident' | 'maintenance';
	title: string;
	/** End of a maintenance, UTC without suffix, when known. */
	until: string | null;
}

function time(value: string | null | undefined): number {
	if (!value) return NaN;
	const normalised = /[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value.replace(' ', 'T') : `${value.replace(' ', 'T')}Z`;
	return new Date(normalised).getTime();
}

/** Incidents first (the worst on top), then maintenances in progress, without repeats. */
export function wallNotices(incidents: readonly Incident[], silences: readonly Silence[], now: number): WallNotice[] {
	const out: WallNotice[] = [];
	const open = incidents.filter((i) => i.status !== 'resolved' && i.status !== 'completed');
	const incidentsOpen = open
		.filter((i) => i.kind === 'incident')
		.sort((a, b) => Number(b.severity === 'major') - Number(a.severity === 'major'));
	for (const i of incidentsOpen) out.push({ key: `i${i.id}`, kind: 'incident', title: i.title, until: null });
	for (const i of open.filter((x) => x.kind === 'maintenance')) {
		const start = time(i.starts_at);
		const end = time(i.ends_at);
		const running = i.status === 'in_progress' || (Number.isFinite(start) && start <= now && (!Number.isFinite(end) || now < end));
		if (running) out.push({ key: `i${i.id}`, kind: 'maintenance', title: i.title, until: i.ends_at });
	}
	for (const s of silences) {
		if (!s.active_now || !s.enabled) continue;
		if (out.some((n) => n.kind === 'maintenance' && n.title === s.name)) continue;
		out.push({ key: `s${s.id}`, kind: 'maintenance', title: s.name, until: s.active_until });
	}
	return out;
}
