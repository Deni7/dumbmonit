import { request } from './client';
import { normalizeCollector } from './types';
import type {
	CollectorInfo,
	DiscoveredDevice,
	DiscoveryResult,
	Target
} from './types';

// --- Network discovery ------------------------------------------------------

interface RawDiscoveredDevice {
	address?: unknown;
	sysname?: unknown;
	sysdescr?: unknown;
	sysobjectid?: unknown;
	suggested_profile?: unknown;
}

function optionalText(value: unknown): string | null {
	return typeof value === 'string' && value.trim() ? value.trim() : null;
}

/**
 * Scans a CIDR looking for SNMP devices (`POST /api/discovery`, admin only).
 *
 * The parameters travel in the body, not the query string: the community is a
 * credential and must not land in server or proxy logs. The server answers
 * `{ devices, scanned }`; this folds each device into the UI shape and marks
 * the ones already monitored (by host, ignoring the port). Throws an
 * `ApiError` with `missing = true` if the route does not exist.
 */
export async function discover(
	cidr: string,
	options: { community?: string; signal?: AbortSignal } = {}
): Promise<DiscoveryResult> {
	const [raw, targets] = await Promise.all([
		request<unknown>('/discovery', {
			method: 'POST',
			body: { cidr: cidr.trim(), community: options.community?.trim() || undefined },
			signal: options.signal,
			anticipated: true
		}),
		// Only used to grey out known addresses: a failure must not hide the scan.
		listTargets(options.signal).catch(() => [] as Target[])
	]);

	const host = (address: string) => address.trim().toLowerCase().replace(/:\d+$/, '');
	const known = new Set(targets.map((t) => host(t.address)));
	const list: unknown[] = Array.isArray(raw)
		? raw
		: raw && typeof raw === 'object' && Array.isArray((raw as { devices?: unknown }).devices)
			? (raw as { devices: unknown[] }).devices
			: [];
	const scanned =
		raw && typeof raw === 'object' && typeof (raw as { scanned?: unknown }).scanned === 'number'
			? (raw as { scanned: number }).scanned
			: null;

	const devices = (list as RawDiscoveredDevice[])
		.filter((item) => item && typeof item.address === 'string' && item.address.trim())
		.map((item): DiscoveredDevice => {
			const address = (item.address as string).trim();
			return {
				address,
				name: optionalText(item.sysname),
				sysname: optionalText(item.sysname),
				description: optionalText(item.sysdescr),
				sysobjectid: optionalText(item.sysobjectid),
				kind: 'snmp',
				profile_id: optionalText(item.suggested_profile),
				already_added: known.has(host(address))
			};
		});

	return { devices, scanned };
}

// --- Device types -----------------------------------------------------------

/**
 * Lists the device types this server can collect, along with their setup notes.
 *
 * Nothing is hard-coded on the interface side: the add screen is built from
 * this response, including for a type this version of the interface does not
 * know yet.
 */
export async function listCollectors(signal?: AbortSignal): Promise<CollectorInfo[]> {
	const raw = await request<CollectorInfo[]>('/collectors', { signal, anticipated: true });
	if (!Array.isArray(raw)) return [];
	return raw.filter((item) => item && typeof item.kind === 'string').map(normalizeCollector);
}
