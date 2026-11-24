import { request } from './client';
import type { OverviewWidgets } from './types';

/** The extra figures of the overview dashboards, over `range` (`1h`, `24h`, `7d`, `30d`). */
export function getOverviewWidgets(range: string, signal?: AbortSignal): Promise<OverviewWidgets> {
	return request<OverviewWidgets>(`/overview/widgets?range=${encodeURIComponent(range)}`, {
		signal,
		anticipated: true
	});
}
