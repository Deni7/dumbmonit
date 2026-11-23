// --- Proxmox VE guests (`crates/server/src/api/proxmox.rs`) -----------------

export type ProxmoxGuestStatus = 'running' | 'stopped' | 'paused' | 'suspended' | 'template' | 'unknown';

/**
 * One row of the Guests panel, as the last probe left it. Every unknown value
 * is `null`, never zero: a VM without guest agent has a disk size but no
 * usage; a stopped guest keeps its sizes and loses its measurements.
 */
export interface ProxmoxGuest {
	vmid: number;
	name: string;
	node: string;
	kind: 'qemu' | 'lxc' | string;
	status: ProxmoxGuestStatus | string;
	/** Of the cores allocated to the guest. */
	cpu_percent: number | null;
	cpu_count: number | null;
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	memory_percent: number | null;
	/**
	 * Memory the guest's process occupies on the host: what the guest sees plus
	 * the emulation overhead. It regularly exceeds the allocated memory.
	 */
	memory_host_bytes: number | null;
	balloon_bytes: number | null;
	disk_used_bytes: number | null;
	disk_total_bytes: number | null;
	disk_percent: number | null;
	/** `true` guest agent answered, `false` enabled but silent, `null` none (or a container). */
	agent: boolean | null;
	network_in_bps: number | null;
	network_out_bps: number | null;
	disk_read_bps: number | null;
	disk_write_bps: number | null;
	uptime_seconds: number | null;
	last_backup_age_seconds: number | null;
	ha_state: string | null;
	/** Cluster pool the guest belongs to, `null` when it is in none. */
	pool: string | null;
	/** Lock held right now (`backup`, `migrate`, …); left on, it blocks every operation. */
	lock: string | null;
	/** Operating system seen from inside, `null` without a guest agent. */
	os: string | null;
	/** First routable address reported from inside, `null` when none is known. */
	ip: string | null;
}

/** One LVM thin pool of a node: a full one puts every guest on it read-only. */
export interface ProxmoxThinPool {
	name: string;
	vg: string;
	used_percent: number | null;
	/** The metadata volume is far smaller and often fills first. */
	metadata_used_percent: number | null;
	size_bytes: number | null;
}

export interface ProxmoxVolumeGroup {
	name: string;
	used_percent: number | null;
	size_bytes: number | null;
}

/** One node of the cluster: is it there, does it hold up, and what is wrong with it. */
export interface ProxmoxNode {
	name: string;
	up: boolean;
	cpu_percent: number | null;
	memory_percent: number | null;
	rootfs_percent: number | null;
	uptime_seconds: number | null;
	version: string | null;
	/** Share of CPU time spent waiting on storage. */
	cpu_iowait_percent: number | null;
	/** Memory reclaimed by KSM by sharing identical pages between guests. */
	ksm_shared_bytes: number | null;
	/** Proxmox packages for which a newer version is available. */
	packages_upgradable: number | null;
	/** A newer kernel than the running one is installed: the node awaits a reboot. */
	reboot_required: boolean | null;
	kernel_running: string | null;
	kernel_installed: string | null;
	/** Core Proxmox daemons that are not running on this node. */
	services_down: string[];
	/** Interfaces set to start at boot that are not up. */
	interfaces_offline: string[];
	thin_pools: ProxmoxThinPool[];
	volume_groups: ProxmoxVolumeGroup[];
}

/** The cluster's nodes, plus what belongs to the cluster rather than to a node. */
export interface ProxmoxNodes {
	nodes: ProxmoxNode[];
	/** Proxmox's own word for the HA watchdog: `armed`, `standby`… `null` before PVE 9. */
	fencing_state: string | null;
	/** `true` when the watchdog will really fence a lost node. */
	fencing_armed: boolean | null;
}

export interface ProxmoxCephOsd {
	name: string;
	host: string;
	device_class: string;
	up: boolean;
	in: boolean;
	used_percent: number | null;
	used_bytes: number | null;
	total_bytes: number | null;
	apply_latency_ms: number | null;
	commit_latency_ms: number | null;
}

export interface ProxmoxCephPool {
	name: string;
	used_percent: number | null;
	used_bytes: number | null;
	size: number | null;
	min_size: number | null;
	pg_num: number | null;
	pg_num_optimal: number | null;
	autoscale: string | null;
}

/**
 * Ceph, when there is one. `available` is false both when the cluster has no
 * Ceph and when it has not been probed yet — the panel simply stays hidden.
 */
export interface ProxmoxCeph {
	available: boolean;
	/** 0 OK, 1 WARN, 2 ERR, 3 unknown. */
	health: number | null;
	health_status: string | null;
	bytes_used: number | null;
	bytes_total: number | null;
	used_percent: number | null;
	osds_total: number | null;
	osds_up: number | null;
	osds_in: number | null;
	osds: ProxmoxCephOsd[];
	pools: ProxmoxCephPool[];
	filesystems: string[];
	/** OSD flags left set (`noout`, `norebalance`, …). */
	flags: string[];
	/** Health checks muted: what `HEALTH_OK` no longer tells you. */
	muted_checks: string[];
}
