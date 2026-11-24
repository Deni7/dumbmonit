import type { TargetId } from './targets.js';

// --- Heartbeat (push) monitors (`crates/server/src/api/push.rs`) -------------

/**
 * `GET /api/targets/{id}/push`: the secret URL a heartbeat device is called
 * on, and what its last call said. Mirrors `PushMonitorView`. The token is
 * returned to admins on purpose (it must be copied into a crontab long after
 * the device was created), and `null` for viewers: it lets a caller say "the
 * job ran", which would hide a failing job.
 */
export interface PushMonitor {
	target_id: TargetId;
	token: string | null;
	/** Path relative to the server: `/api/push/<token>`. The UI prepends its origin. `null` for viewers. */
	path: string | null;
	last_seen_at: string | null;
	/** Age of the last call in seconds, `null` until the first one. */
	last_seen_age_secs: number | null;
	/** What the last call declared. */
	last_status: 'up' | 'down';
	last_message: string;
	received_total: number;
	created_at: string;
	/** Effective settings, `null` when the options are unreadable (`settings_error`). */
	expected_interval_secs: number | null;
	grace_secs: number | null;
	settings_error: string | null;
	verdict: 'waiting' | 'on_time' | 'missed' | 'reported_down';
	/** Cron expression replacing the interval, and the IANA zone it is read in. */
	schedule: string | null;
	timezone: string | null;
	/** Next run of a cron schedule, UTC without suffix. */
	next_expected_at: string | null;
	max_duration_secs: number | null;
	/** The latest run, open or finished. */
	last_run: PushRun | null;
}

/** One run of a heartbeat job. Mirrors `PushRunView`. */
export interface PushRun {
	id: number;
	/** UTC without suffix; `null` when no `/start` preceded the end. */
	started_at: string | null;
	/** `null` while the run is open. */
	finished_at: string | null;
	/** `null` when the duration is unknown. */
	duration_secs: number | null;
	/** `lost`: started, then replaced by a new start without ever finishing. */
	status: 'running' | 'success' | 'failed' | 'lost';
	exit_code: number | null;
	/** Output of the job, truncated server-side to its last 10 KiB. */
	output: string;
}

/** `GET /api/targets/{id}/push/runs`. Mirrors `PushRunsPage`. */
export interface PushRunsPage {
	runs: PushRun[];
	/** Pass as `before` to get the next page; `null` on the last one. */
	next_before: number | null;
}

// --- Proxmox Datacenter Manager (crates/server/src/api/pdm.rs) --------------
//
// Times are Unix seconds, as PDM reports them (not server date strings).
// Every measurement is nullable: an instance the console cannot reach says
// nothing, and nothing is not zero.

/** Totals the console computes for the whole estate (`/resources/status`). */
export interface PdmEstate {
	remotes: number | null;
	remotes_failed: number | null;
	nodes_online: number | null;
	nodes_offline: number | null;
	qemu_running: number | null;
	qemu_stopped: number | null;
	lxc_running: number | null;
	lxc_stopped: number | null;
	cpu_used_cores: number | null;
	cpu_total_cores: number | null;
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	storage_used_bytes: number | null;
	storage_total_bytes: number | null;
	datastores: number | null;
}

/** One federated Proxmox VE cluster or Proxmox Backup Server. */
export interface PdmRemote {
	id: string;
	/** `pve` or `pbs`, when the console says so. */
	kind: string | null;
	reachable: boolean;
	/** What the console got back when it failed, verbatim. */
	error: string | null;
	version: string | null;
	/** Another instance of the same product runs a newer version. */
	version_behind: boolean;
	nodes: string[];
	nodes_online: number | null;
	nodes_offline: number | null;
	guests_running: number | null;
	guests_stopped: number | null;
	cpu_used_cores: number | null;
	cpu_total_cores: number | null;
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	storage_used_bytes: number | null;
	storage_total_bytes: number | null;
	datastores: number | null;
	/** `none`, `unknown`, `mixed` or `active`. */
	subscription: string | null;
	last_collection: number | null;
	updates_pending: number | null;
	tasks_failed: number;
	memory_used_percent: number | null;
	storage_used_percent: number | null;
}

export interface PdmRemotes {
	probed_at: number | null;
	version: string | null;
	estate: PdmEstate;
	remotes: PdmRemote[];
}

export type PdmTaskKind =
	| 'backup'
	| 'migrate'
	| 'sync'
	| 'verify'
	| 'prune'
	| 'gc'
	| 'replication'
	| 'update'
	| 'other';

export interface PdmFailure {
	upid: string;
	/** Federated instance the task ran on; empty for a task of the console itself. */
	remote: string;
	worker_type: string;
	kind: PdmTaskKind;
	worker_id: string;
	node: string | null;
	user: string | null;
	start: number;
	end: number | null;
	/** Error message, without the `TASK ERROR:` prefix. */
	error: string;
}

export interface PdmCertificate {
	filename: string;
	subject: string | null;
	issuer: string | null;
	not_after: number | null;
}

export interface PdmSubscription {
	status: string | null;
	message: string | null;
	active_nodes: number | null;
	total_nodes: number | null;
}

export interface PdmNode {
	cpu_percent: number | null;
	cpu_count: number | null;
	iowait_percent: number | null;
	cpu_model: string | null;
	load1: number | null;
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	swap_used_bytes: number | null;
	swap_total_bytes: number | null;
	rootfs_used_bytes: number | null;
	rootfs_total_bytes: number | null;
	uptime_seconds: number | null;
	kernel: string | null;
	updates_pending: number | null;
	certificates: PdmCertificate[];
	subscription: PdmSubscription | null;
}

export interface PdmHealth {
	probed_at: number | null;
	version: string | null;
	/** `null` when the console host is not read (option off, or missing privilege). */
	node: PdmNode | null;
	memory_used_percent: number | null;
	rootfs_used_percent: number | null;
	/** Soonest expiry first. */
	certificates: PdmCertificate[];
	subscription: PdmSubscription | null;
}

// ---------------------------------------------------------------------------
// Skipped optional features (`crates/server/src/api/skipped_features.rs`)
// ---------------------------------------------------------------------------

export interface SkippedFeature {
	/** `guest_agent`, `updates`, `certificates`, `disks`, `services`, `traffic_control`, `remote_updates`. */
	feature: string;
	/** The privilege the token lacks, as the device names it. */
	privilege: string;
}

export interface SkippedFeatures {
	features: SkippedFeature[];
}

// ---------------------------------------------------------------------------
// Proxmox Mail Gateway (`crates/server/src/api/pmg.rs`)
// ---------------------------------------------------------------------------

export interface PmgQueueDomain {
	domain: string;
	messages: number;
}

export interface PmgQueue {
	/** `incoming`, `active`, `deferred` or `hold`. */
	queue: string;
	messages: number;
	domains: number;
	/**
	 * Lower bound on the age of the oldest message, in seconds: qshape reports
	 * age brackets, so this is the floor of the highest occupied one. `null`
	 * when the queue is empty.
	 */
	oldest_age_seconds: number | null;
	top_domains: PmgQueueDomain[];
	/** True when the oldest message has been waiting for more than four hours. */
	stuck: boolean;
}

export interface PmgQueues {
	probed_at: number | null;
	/** In reading order: incoming, active, deferred, hold. */
	queues: PmgQueue[];
	total_messages: number;
	stuck: boolean;
}

/** Totals since local midnight on the gateway, as PMG aggregates them. */
export interface PmgMail {
	count_in: number | null;
	count_out: number | null;
	bytes_in: number | null;
	bytes_out: number | null;
	spam_in: number | null;
	spam_out: number | null;
	virus_in: number | null;
	virus_out: number | null;
	bounces_in: number | null;
	bounces_out: number | null;
	junk_in: number | null;
	junk_out: number | null;
	greylisted: number | null;
	spf_rejects: number | null;
	rbl_rejects: number | null;
	pregreet_rejects: number | null;
	avg_processing_seconds: number | null;
}

export interface PmgRecentPoint {
	/** Start of the slice, Unix seconds. */
	time: number;
	timespan: number;
	count_in: number;
	count_out: number;
	spam_in: number;
	virus_in: number;
}

export interface PmgSpamScore {
	/** `0` to `10`; `10` aggregates everything above. */
	level: string;
	count: number;
	ratio_percent: number | null;
}

export interface PmgVirus {
	name: string;
	count: number;
}

/** Counts only: no subject, sender or message content ever leaves the gateway. */
export interface PmgQuarantine {
	spam_count: number | null;
	spam_bytes: number | null;
	spam_avg_level: number | null;
	virus_count: number | null;
	virus_bytes: number | null;
	/** `null` unless the attachment quarantine option is on. */
	attachment_count: number | null;
}

export interface PmgTraffic {
	probed_at: number | null;
	mail: PmgMail | null;
	/** Oldest slice first. */
	recent: PmgRecentPoint[];
	spam_scores: PmgSpamScore[];
	viruses: PmgVirus[];
	quarantine: PmgQuarantine | null;
}

export interface PmgService {
	service: string;
	description: string | null;
	state: string | null;
	unit_state: string | null;
	running: boolean;
}

export interface PmgSignature {
	/** `main`, `daily`, `bytecode` for ClamAV; the channel for SpamAssassin. */
	name: string;
	version: string | null;
	updated_at: number | null;
	signatures: number | null;
	update_available: boolean | null;
	/** `virus` (ClamAV) or `spam` (SpamAssassin). */
	family: string;
	/** `null` when the database has never been dated. */
	age_seconds: number | null;
	stale: boolean;
}

export interface PmgCertificate {
	filename: string;
	subject: string | null;
	issuer: string | null;
	not_after: number | null;
	san: string[];
}

export interface PmgSubscription {
	status: string;
	level: string | null;
	next_due_date: string | null;
}

export interface PmgNode {
	name: string;
	uptime_seconds: number | null;
	cpu_percent: number | null;
	cpu_count: number | null;
	loadavg: number[];
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	swap_used_bytes: number | null;
	swap_total_bytes: number | null;
	rootfs_used_bytes: number | null;
	rootfs_total_bytes: number | null;
	kernel: string | null;
	/** API version on this node: in a cluster, the one left behind shows here. */
	version: string | null;
	insync: boolean | null;
	clock_offset_seconds: number | null;
	services: PmgService[];
	virus_databases: PmgSignature[];
	spam_rules: PmgSignature[];
	certificates: PmgCertificate[];
	subscription: PmgSubscription | null;
	updates_pending: number | null;
	updates_security_pending: number | null;
	/** Virus databases then spam rules, each dated and judged. */
	signatures: PmgSignature[];
	/** Certificates expiring in less than fourteen days. */
	expiring_certificates: PmgCertificate[];
}

export interface PmgClusterNode {
	name: string;
	ip: string | null;
	/** `master` or `node`. */
	role: string | null;
	insync: boolean | null;
	error: string | null;
}

export interface PmgStoppedService {
	node: string;
	service: string;
	description: string | null;
	state: string | null;
}

export interface PmgHealth {
	probed_at: number | null;
	version: string | null;
	nodes: PmgNode[];
	/** Empty on a standalone gateway: no cluster, not a degraded one. */
	cluster: PmgClusterNode[];
	stopped_services: PmgStoppedService[];
}
