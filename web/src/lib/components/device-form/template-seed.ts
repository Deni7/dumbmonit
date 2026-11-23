/**
 * A monitor template as the form's seed (`TargetForm` with `template`): the
 * same path as a copy, with no name, no address and no credential to carry —
 * a template never holds them. Where the new monitor goes (relay, location,
 * folder) comes from the page, as for any new monitor.
 */
import type { Target } from '#lib/api/index.js';
import type { MonitorTemplate } from '#lib/api/monitorTemplates.js';

export interface SeedDefaults {
	viaAgent: number | null;
	location: number | null;
	folder: string;
}

export function templateSeed(template: MonitorTemplate, defaults: SeedDefaults): Target {
	return {
		id: 0,
		name: '',
		address: '',
		kind: template.kind,
		profile_id: null,
		parent_id: null,
		via_agent: defaults.viaAgent,
		multisite: null,
		interval_secs: template.interval_secs,
		enabled: true,
		cve_scan: true,
		tags: { ...template.tags },
		credential_kind: '',
		has_secret: false,
		credential_public: {},
		group_name: defaults.folder,
		note: '',
		channels: [...template.channels],
		position: 0,
		location_id: defaults.location,
		last_probe_at: null,
		last_error: null,
		error_kind: null
	};
}

/** The same channels, whatever their order: what decides whether a save sends them. */
export function sameChannels(a: readonly number[], b: readonly number[]): boolean {
	const key = (ids: readonly number[]) => [...new Set(ids)].sort((x, y) => x - y).join(',');
	return key(a) === key(b);
}
