// --- Multi-product network discovery -----------------------------------------
// Mirrors `api/discovery.rs` (jobs). Dates are UTC without suffix.

export type DiscoveryJobStatus = 'running' | 'done' | 'failed' | 'cancelled';
export type DiscoveryPhase = 'sweep' | 'fingerprint' | 'done';
export type DiscoveryDepth = 'standard' | 'extended';
export type DiscoveryConfidence = 'high' | 'medium' | 'low';

export interface DiscoveryJobRequest {
	cidr: string;
	/** Id of the relay agent that scans, `null` for the server itself. */
	agent_id: number | null;
	/** Secret: sent, never returned. */
	snmp_community?: string;
	depth: DiscoveryDepth;
}

export interface DiscoveryJobSummary {
	id: string;
	cidr: string;
	agent_id: number | null;
	agent_name: string | null;
	status: DiscoveryJobStatus;
	started_at: string;
	finished_at: string | null;
	hosts_found: number;
}

export interface DiscoveredService {
	port: number;
	protocol: 'tcp' | 'udp';
	/** Readable product label: "Proxmox VE", "SSH", "HTTP". */
	product: string;
	/** Collector kind that monitors this product, `null` when none does. */
	kind: string | null;
	version: string | null;
	confidence: DiscoveryConfidence;
	tls: boolean;
	detail: string | null;
	/** Existing target already pointing at this ip(+port). */
	monitored_target_id: string | null;
	/** Fields to pre-fill in the target form for `kind`; never a secret. */
	prefill: Record<string, unknown> | null;
}

export interface DiscoveredHost {
	ip: string;
	hostname: string | null;
	mac: string | null;
	vendor: string | null;
	services: DiscoveredService[];
}

export interface DiscoveryJob extends DiscoveryJobSummary {
	progress: { phase: DiscoveryPhase; probed: number; total: number };
	error: string | null;
	hosts: DiscoveredHost[];
}

// --- Scheduled network scans and inventory -----------------------------------
// Mirrors `api/netinventory.rs`. Dates are UTC without suffix.

export type NetscanRunStatus = 'running' | 'done' | 'failed' | 'cancelled';

export interface NetscanScheduleInput {
	name: string;
	cidr: string;
	/** Id of the relay agent that scans, `null` for the server itself. */
	agent_id: number | null;
	/** 300 (5 min) to 2 592 000 (30 days). */
	interval_secs: number;
	depth: DiscoveryDepth;
	enabled: boolean;
}

export interface NetscanSchedule extends NetscanScheduleInput {
	id: number;
	agent_name: string | null;
	/** The first scan has built the inventory (nothing was reported). */
	initialized: boolean;
	running: boolean;
	last_run_at: string | null;
	last_status: NetscanRunStatus | null;
	last_error: string | null;
	devices: number;
	unknown_devices: number;
	/** Devices the administrator chose to ignore. */
	ignored_devices: number;
	/** New devices on the address of a known device under another MAC, still to review. */
	mac_changes: number;
	/** Unknown ports of known devices. */
	unknown_ports: number;
}

export interface NetscanPort {
	id: number;
	port: number;
	protocol: 'tcp' | 'udp';
	product: string;
	/** Monitor type the detection proposes for this service, when it recognised one. */
	kind: string | null;
	known: boolean;
	first_seen: string;
	last_seen: string;
}

export interface NetscanDevice {
	id: number;
	schedule_id: number;
	ip: string;
	mac: string | null;
	hostname: string | null;
	/** Manufacturer from the MAC prefix (OUI), when known. */
	vendor: string | null;
	/** Also true for an ignored device. */
	known: boolean;
	ignored: boolean;
	/** MAC the address had before this new device took it (spoofing or replaced hardware). */
	mac_change_from: string | null;
	first_seen: string;
	last_seen: string;
	ports: NetscanPort[];
}

/** Mirrors `GET /api/discovery/review`. */
export interface NetscanReview {
	/** New devices that are neither monitored nor ignored. */
	to_review: number;
	/** The latest batch of new devices was not acknowledged yet: announce it once. */
	announce: boolean;
	/** New devices in that batch. */
	batch_count: number;
	batch: number;
}

export type NetscanBulkAction = 'add' | 'ignore' | 'restore';

export type NetscanBulkOutcome = 'added' | 'pending_credentials' | 'already_present' | 'ignored' | 'restored' | 'error';

export interface NetscanBulkResult {
	device_id: number;
	outcome: NetscanBulkOutcome;
	/** The monitor created, or the one that already existed. */
	target_id: number | null;
	/** English text from the server (an error, mostly). */
	message: string | null;
}

/** Mirrors `POST /api/discovery/inventory/bulk` (200 devices at most per call). */
export interface NetscanBulkReport {
	results: NetscanBulkResult[];
	added: number;
	pending_credentials: number;
	already_present: number;
	errors: number;
}
