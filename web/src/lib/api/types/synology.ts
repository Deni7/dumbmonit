import type { TargetId } from './targets.js';

// --- Synology DSM -------------------------------------------------------------
// Mirrors `crates/server/src/api/synology.rs`: the device panel reads the last
// `dumbmonit_synology_*` series and the Active Backup history the probe stored.

export interface SynologySystem {
	model: string | null;
	dsm_version: string | null;
	uptime_seconds: number | null;
	temperature_celsius: number | null;
	temperature_warning: boolean | null;
	cpu_percent: number | null;
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	memory_percent: number | null;
	/** Worst state seen: 0 normal, 1 attention, 2 critical. */
	storage_health: number | null;
	system_crashed: boolean | null;
	system_need_repair: boolean | null;
}

export interface SynologyVolume {
	id: string;
	name: string;
	fs_type: string;
	raid_type: string;
	/** DSM's own word: `normal`, `degrade`, `crashed`, `repairing`… */
	status: string;
	/** 0 normal, 1 attention, 2 critical. */
	severity: number;
	total_bytes: number | null;
	used_bytes: number | null;
	used_percent: number | null;
	/** Usage without the space reserved by thick iSCSI LUNs: what the fill alert reads. */
	effective_used_percent?: number | null;
	/** Sum of the thick LUN sizes on the volume; `null` without LUNs. */
	lun_reserved_bytes?: number | null;
}

export interface SynologyDisk {
	id: string;
	name: string;
	model: string;
	serial: string;
	vendor: string;
	firmware: string;
	kind: string;
	ssd: boolean;
	status: string;
	severity: number;
	smart_status: string;
	smart_severity: number;
	temperature_celsius: number | null;
	size_bytes: number | null;
	bad_sector_exceeded: boolean | null;
	life_below_threshold: boolean | null;
	remaining_life_percent: number | null;
	/** DSM's `unc` counter: unreadable sectors. */
	unc_count: number | null;
}

/** A storage pool or an SSD cache: DSM gives both the same shape. */
export interface SynologyPool {
	id: string;
	name: string;
	/** DSM's own word: `shr_1`, `raid_5`, `basic`… */
	raid_type: string;
	/** DSM's own word: `normal`, `degrade`, `crashed`, `repairing`… */
	status: string;
	/** 0 normal, 1 attention, 2 critical. */
	severity: number;
	/** Disks of the group DSM reports as failed. */
	failed_disks: number | null;
	total_bytes: number | null;
	/** Share already handed to volumes; DSM gives none for an SSD cache. */
	used_bytes: number | null;
}

export interface SynologyOverview {
	system: SynologySystem;
	volumes: SynologyVolume[];
	/** Storage pools: redundancy lives here, not in the volume. */
	pools: SynologyPool[];
	/** SSD caches; empty on a NAS without one. */
	ssd_caches: SynologyPool[];
	disks: SynologyDisk[];
	/** Unix seconds of the newest reading, `null` before the first probe. */
	sampled_at: number | null;
}

// --- iSCSI LUNs and zvols -------------------------------------------------------
// Mirrors `crates/server/src/api/luns.rs`: `GET /targets/{id}/luns`.

export type LunProvisioning = 'thick' | 'thin' | 'unknown';

export interface LunTarget {
	iqn: string;
	name: string;
	sessions: number;
	/** IQNs of the connected initiators. */
	initiators: string[];
}

/** The monitor that uses a LUN, and its own fill level. */
export interface LunConsumer {
	/** `manual` = saved link, `auto` = unambiguous match on name and size. */
	source: 'manual' | 'auto';
	target_id: TargetId;
	target_name: string;
	family: string;
	family_label: string;
	entity: string;
	used_percent: number | null;
	used_bytes: number | null;
	size_bytes: number | null;
}

export interface Lun {
	/** DSM UUID or zvol name: the `lun` label. */
	id: string;
	name: string;
	/** Host volume (Synology) or pool (TrueNAS). */
	host_volume: string;
	provisioning: LunProvisioning;
	/** DSM's own word: `BLUN_THICK`, `BLUN`, `FILE`…; empty for a zvol. */
	lun_type: string;
	status: string;
	size_bytes: number | null;
	/** Space taken on the volume: the size if thick, the written space if thin. */
	reserved_bytes: number | null;
	allocated_bytes: number | null;
	/** Written share of a thin LUN; `null` for a thick one (opaque from the NAS). */
	used_percent: number | null;
	mapped: boolean | null;
	sessions: number | null;
	targets: LunTarget[];
	consumer: LunConsumer | null;
}

export interface LunVolume {
	volume: string;
	name: string;
	total_bytes: number | null;
	used_bytes: number | null;
	used_percent: number | null;
	lun_count: number | null;
	/** Sum of the thick LUN sizes. */
	reserved_bytes: number | null;
	thin_allocated_bytes: number | null;
	thin_provisioned_bytes: number | null;
	/** Promised to thin LUNs and not written yet. */
	thin_promised_bytes: number | null;
	/** Data outside LUNs. */
	data_bytes: number | null;
	effective_total_bytes: number | null;
	effective_used_bytes: number | null;
	effective_used_percent: number | null;
	/** Promised thin space as a percentage of the free space. */
	overcommit_percent: number | null;
	fully_reserved: boolean | null;
}

export interface TargetLuns {
	luns: Lun[];
	/** Synology volumes hosting at least one LUN; empty elsewhere. */
	volumes: LunVolume[];
	sampled_at: number | null;
}

export interface LunLinkPayload {
	target_id: TargetId;
	family: string;
	entity: string;
}

export type AbbDeviceState = 'ok' | 'idle' | 'learning' | 'running' | 'overdue' | 'failing' | 'never';

export type AbbDayOutcome = 'success' | 'failure' | 'cancelled' | 'running' | 'none';

export interface AbbDayCell {
	/** Local day, `YYYY-MM-DD`. */
	day: string;
	outcome: AbbDayOutcome;
	runs: number;
}

export interface AbbDevice {
	device_id: number;
	device_name: string;
	task_id: number;
	task_name: string;
	state: AbbDeviceState;
	last_success_s: number | null;
	last_run_s: number | null;
	last_outcome: string | null;
	runs_30d: number;
	successes_30d: number;
	failures_30d: number;
	consecutive_failures: number;
	typical_interval_s: number | null;
	p90_gap_s: number | null;
	/** Active time tolerated since the last success before "overdue". */
	allowance_s: number;
	active_elapsed_s: number | null;
	/** Monday first. */
	active_weekdays: [boolean, boolean, boolean, boolean, boolean, boolean, boolean];
	usual_hour: number | null;
	/** The rhythm in words: "weekdays, around 20:00". */
	rhythm: string;
	/** Thirty cells, oldest first. */
	calendar: AbbDayCell[];
}

export interface AbbTask {
	task_id: string;
	name: string;
	source_type: string;
	result: string;
	/** 1 success, 0 failed, 2 running, -1 unknown. */
	last_status: number | null;
	enabled: boolean | null;
	device_count: number | null;
	last_success_seconds: number | null;
}

export interface SynologyAbb {
	tasks: AbbTask[];
	devices: AbbDevice[];
	/** Offset, in seconds, in which the rhythm's days and hours are expressed (the server's local time). */
	utc_offset_s: number;
	min_allowance_s: number;
	learning_allowance_s: number;
	failing_streak: number;
}
