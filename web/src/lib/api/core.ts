import { request, ApiError } from './client';
import type {
	Health,
	LocalSubnet,
	MetricSeries,
	NetconfigCompare,
	NetconfigOverview,
	NetconfigVersionContent,
	ProbeLocationsReport,
	ProbeReport,
	Target,
	TargetId,
	TargetPayload,
	TargetTestReport,
	TargetTestRequest,
	WebchangeChange,
	WebchangeChangeDetail,
	WebchangeChangeKind,
	WebchangePagesResponse
} from './types';

// --- Health -----------------------------------------------------------------

export function getHealth(signal?: AbortSignal): Promise<Health> {
	return request<Health>('/health', { signal });
}

// --- Targets ----------------------------------------------------------------

export function listTargets(signal?: AbortSignal): Promise<Target[]> {
	return request<Target[]>('/targets', { signal });
}

export function getTarget(id: TargetId, signal?: AbortSignal): Promise<Target> {
	return request<Target>(`/targets/${id}`, { signal });
}

export function getProbeLocations(id: TargetId, signal?: AbortSignal): Promise<ProbeLocationsReport> {
	return request<ProbeLocationsReport>(`/targets/${id}/probe-locations`, { signal });
}

export function createTarget(payload: TargetPayload): Promise<Target> {
	return request<Target>('/targets', { method: 'POST', body: payload });
}

export function updateTarget(id: TargetId, payload: TargetPayload): Promise<Target> {
	return request<Target>(`/targets/${id}`, { method: 'PUT', body: payload });
}

/** Probes the entered address and credentials once; nothing is saved. */
export function testTarget(
	payload: TargetTestRequest,
	signal?: AbortSignal
): Promise<TargetTestReport> {
	return request<TargetTestReport>('/targets/test', { method: 'POST', body: payload, signal });
}

/** Suggested networks for the scan form (configured, from devices, from the server). */
export async function localSubnets(signal?: AbortSignal): Promise<LocalSubnet[]> {
	const raw = await request<{ subnets?: LocalSubnet[] }>('/discovery/subnets', {
		signal,
		anticipated: true
	});
	return Array.isArray(raw?.subnets) ? raw.subnets : [];
}

export function deleteTarget(id: TargetId): Promise<void> {
	return request<void>(`/targets/${id}`, { method: 'DELETE' });
}

/** Hands a monitor managed by Docker labels back to manual management. */
export function releaseManagedTarget(id: TargetId): Promise<void> {
	return request<void>(`/targets/${id}/release`, { method: 'POST' });
}

/**
 * Sets the manual rank of each listed device to its index in `order`.
 * Devices left out keep their rank — a local reorder (one folder, one
 * Up/Down click) does not need to resend every device.
 */
export function reorderTargets(order: TargetId[]): Promise<void> {
	return request<void>('/targets/reorder', { method: 'POST', body: { order } });
}

/**
 * Renames a folder server-side: every device in it moves, and the status pages
 * that ticked the folder follow the new name.
 */
export function renameTargetFolder(from: string, to: string): Promise<void> {
	return request<void>('/targets/folders/rename', { method: 'POST', body: { from, to } });
}

/** Probes the device right away. Can be slow: the caller shows an indicator. */
export function probeTarget(id: TargetId): Promise<ProbeReport> {
	return request<ProbeReport>(`/targets/${id}/probe`, { method: 'POST' });
}

// --- Website changes (webchange) --------------------------------------------

/** Every page this `webchange` device watches, as last probed. */
export function getWebchangePages(id: TargetId, signal?: AbortSignal): Promise<WebchangePagesResponse> {
	return request<WebchangePagesResponse>(`/targets/${id}/webchange/pages`, { signal });
}

/** Changes detected on this device, newest first; `url` narrows to one page. */
export function listWebchangeChanges(
	id: TargetId,
	options: { url?: string; limit?: number; kind?: WebchangeChangeKind; before?: number } = {},
	signal?: AbortSignal
): Promise<WebchangeChange[]> {
	return request<WebchangeChange[]>(`/targets/${id}/webchange/changes`, {
		query: { url: options.url, limit: options.limit, kind: options.kind, before: options.before },
		signal
	});
}

/** One change with its before/after snapshots and text diff. */
export function getWebchangeChange(
	id: TargetId,
	changeId: number,
	signal?: AbortSignal
): Promise<WebchangeChangeDetail> {
	return request<WebchangeChangeDetail>(`/targets/${id}/webchange/changes/${changeId}`, { signal });
}

/** Asks for an immediate check of every page of this device. Fire-and-forget: 202. */
export function checkWebchangeNow(id: TargetId): Promise<void> {
	return request<void>(`/targets/${id}/webchange/check`, { method: 'POST' });
}

// --- Network configuration backups (netconfig) ------------------------------

/** State, host key and version list of a `netconfig` device. */
export function getNetconfig(id: TargetId, signal?: AbortSignal): Promise<NetconfigOverview> {
	return request<NetconfigOverview>(`/targets/${id}/netconfig`, { signal });
}

/** One version's text, secrets masked unless `unmasked` (administrators only, audited). */
export function getNetconfigVersion(
	id: TargetId,
	versionId: number,
	unmasked = false,
	signal?: AbortSignal
): Promise<NetconfigVersionContent> {
	return request<NetconfigVersionContent>(`/targets/${id}/netconfig/versions/${versionId}`, {
		query: { unmasked: unmasked || undefined },
		signal
	});
}

/** Line diff between two versions; `from` defaults to the version before `to`. */
export function compareNetconfig(
	id: TargetId,
	to: number,
	options: { from?: number; unmasked?: boolean } = {},
	signal?: AbortSignal
): Promise<NetconfigCompare> {
	return request<NetconfigCompare>(`/targets/${id}/netconfig/diff`, {
		query: { to, from: options.from, unmasked: options.unmasked || undefined },
		signal
	});
}

/** Download link of a raw version (administrators signed in only, audited). */
export function netconfigDownloadUrl(id: TargetId, versionId: number): string {
	return `/api/targets/${id}/netconfig/versions/${versionId}/download`;
}

/** Reads the configuration now. Fire-and-forget: 202, `started` false if one is running. */
export function fetchNetconfigNow(id: TargetId): Promise<{ started: boolean }> {
	return request<{ started: boolean }>(`/targets/${id}/netconfig/fetch`, { method: 'POST' });
}

/** Accepts the new SSH host key the device presents, after checking its fingerprint. */
export function acceptNetconfigHostKey(id: TargetId, fingerprint: string): Promise<void> {
	return request<void>(`/targets/${id}/netconfig/host-key/accept`, {
		method: 'POST',
		body: { fingerprint }
	});
}

// --- Metrics (anticipated) --------------------------------------------------

export interface QueryRangeParams {
	query: string;
	/** Bounds in milliseconds, as expected by the server. */
	start: number;
	end: number;
	/** Sampling step in seconds. */
	step: number;
}

/**
 * Queries VictoriaMetrics over a range. Returns an empty array while the route
 * is not deployed, so that a missing chart does not keep the page from living.
 */
export async function queryRange(
	params: QueryRangeParams,
	signal?: AbortSignal
): Promise<MetricSeries[]> {
	try {
		const series = await request<MetricSeries[]>('/metrics/query_range', {
			query: {
				query: params.query,
				// Whole seconds: two views asking for "the last hour" in the same
				// second send the same URL, and share one request (see `flight.ts`).
				start: Math.floor(params.start / 1000) * 1000,
				end: Math.floor(params.end / 1000) * 1000,
				step: Math.max(1, Math.round(params.step))
			},
			signal,
			anticipated: true
		});
		return Array.isArray(series) ? series : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

/**
 * Queries VictoriaMetrics at the present instant (`GET /api/metrics/query`).
 *
 * Each returned series carries a single value in `values`. Like `queryRange`,
 * the function returns an empty array if the route is not deployed: an unknown
 * state beats a page in error.
 */
export async function queryInstant(query: string, signal?: AbortSignal): Promise<MetricSeries[]> {
	try {
		const series = await request<MetricSeries[]>('/metrics/query', {
			query: { query },
			signal,
			anticipated: true
		});
		return Array.isArray(series) ? series : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}
