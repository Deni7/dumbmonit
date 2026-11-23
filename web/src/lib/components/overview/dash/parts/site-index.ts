/**
 * Which site and kind a monitor belongs to, from `GET /api/overview/widgets`
 * (`sites`), for the widgets that read monitors from the app-wide store.
 * Pure on purpose (types only), so it runs under `node --test`.
 */
import type { WidgetCategory, WidgetSite } from '#lib/api/types/overview.js';

export interface MonitorPlace {
	siteKey: string;
	siteName: string;
	kind: string;
	category: WidgetCategory;
}

export function siteIndex(sites: readonly WidgetSite[]): Map<number, MonitorPlace> {
	const index = new Map<number, MonitorPlace>();
	for (const site of sites) {
		for (const monitor of site.monitors) {
			index.set(monitor.id, { siteKey: site.key, siteName: site.name, kind: monitor.kind, category: monitor.category });
		}
	}
	return index;
}

/**
 * Monitors gathered by site, in the order of `sites`; a monitor the sites do
 * not know yet (added since the last read) goes to the first site, or to a
 * lone group named `fallback` when there is no site at all.
 */
export function groupBySite<T extends { id: number }>(
	monitors: readonly T[],
	sites: readonly WidgetSite[],
	fallback: string
): { key: string; name: string; monitors: T[] }[] {
	const index = siteIndex(sites);
	const groups = sites.map((site) => ({ key: site.key, name: site.name, monitors: [] as T[] }));
	const byKey = new Map(groups.map((g) => [g.key, g]));
	let spare: { key: string; name: string; monitors: T[] } | undefined = groups[0];
	for (const monitor of monitors) {
		const place = index.get(monitor.id);
		const group = place ? byKey.get(place.siteKey) : undefined;
		if (group) {
			group.monitors.push(monitor);
			continue;
		}
		if (!spare) {
			spare = { key: 'local', name: fallback, monitors: [] };
			groups.push(spare);
		}
		spare.monitors.push(monitor);
	}
	return groups.filter((g) => g.monitors.length > 0);
}
