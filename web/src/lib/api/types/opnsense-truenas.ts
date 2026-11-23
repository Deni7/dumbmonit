// ---------------------------------------------------------------------------
// OPNsense firewall (`crates/server/src/api/opnsense.rs`)
// ---------------------------------------------------------------------------

/**
 * One gateway and what dpinger says about it. The server flattens its
 * `GatewayView` into the row, so the judgement flags sit beside the readings.
 */
export interface OpnsenseGatewayRow {
	name: string;
	address: string | null;
	/** `online`, `down`, `loss`, `delay`, `force_down`, or `unknown` when unmonitored. */
	status: string;
	/** Round-trip delay in milliseconds. `null` when the gateway is not monitored. */
	delay_ms: number | null;
	loss_percent: number | null;
	stddev_ms: number | null;
	/** True when dpinger actually watches this gateway. */
	monitored: boolean;
	/** True for the default gateway of its address family. */
	default_gateway: boolean;
	/** True when dpinger calls the gateway down (`down` or `force_down`). */
	down: boolean;
	/** True when it still answers but badly: loss or latency reported. */
	degraded: boolean;
	slow: boolean;
	lossy: boolean;
}

/** An address carried by an interface that has a gateway: the WAN address of the moment. */
export interface OpnsenseWanAddress {
	interface: string;
	address: string;
}

export interface OpnsenseGateways {
	probed_at: number | null;
	/** The ones doing badly first, then by name. */
	gateways: OpnsenseGatewayRow[];
	down: number;
	degraded: number;
	wan_addresses: OpnsenseWanAddress[];
}

/** An interface and its counters, with the label the page should print. */
export interface OpnsenseInterfaceRow {
	device: string;
	/** OPNsense identifier (`wan`, `lan`, `opt1`); absent when unnamed by the configuration. */
	identifier: string | null;
	description: string | null;
	up: boolean | null;
	addresses: string[];
	media: string | null;
	/** True when a gateway leaves through this interface: that is what marks a way out. */
	has_gateway: boolean;
	bytes_in: number | null;
	bytes_out: number | null;
	packets_in: number | null;
	packets_out: number | null;
	errors_in: number | null;
	errors_out: number | null;
	drops: number | null;
	collisions: number | null;
	/** Description, identifier or device — whichever the firewall gave first. */
	label: string;
	/** True when the interface carries errors or dropped packets. */
	faulty: boolean;
}

/** The pf state table. */
export interface OpnsenseFirewallRow {
	states: number | null;
	state_limit: number | null;
	states_used_percent: number | null;
	source_nodes: number | null;
	enabled: boolean | null;
	/** True when the table is filling up enough to deserve a look. */
	busy: boolean;
}

/** Leases of one DHCP backend. Counted, never listed. */
export interface OpnsenseDhcp {
	/** `kea`, `isc` or `dnsmasq`. */
	backend: string;
	total: number | null;
	active: number | null;
}

export interface OpnsenseTraffic {
	probed_at: number | null;
	/** Named interfaces first, then by device. */
	interfaces: OpnsenseInterfaceRow[];
	firewall: OpnsenseFirewallRow | null;
	dhcp: OpnsenseDhcp[];
}

export interface OpnsenseDisk {
	device: string;
	mountpoint: string | null;
	used_bytes: number | null;
	total_bytes: number | null;
	used_percent: number | null;
}

export interface OpnsenseTemperature {
	sensor: string;
	celsius: number;
}

/** The machine itself. */
export interface OpnsenseSystem {
	uptime_seconds: number | null;
	cpu_percent: number | null;
	cpu_count: number | null;
	cpu_model: string | null;
	/** The three load averages, when the firewall gives them. */
	load: number[];
	memory_used_bytes: number | null;
	memory_total_bytes: number | null;
	memory_used_percent: number | null;
	swap_used_bytes: number | null;
	swap_total_bytes: number | null;
	swap_used_percent: number | null;
	/** FreeBSD network buffers: exhausting them stops routing with no other symptom. */
	mbuf_used: number | null;
	mbuf_total: number | null;
	mbuf_used_percent: number | null;
	/** Buffer allocations refused since boot: above zero, the ceiling was already hit once. */
	mbuf_failures: number | null;
	disks: OpnsenseDisk[];
	temperatures: OpnsenseTemperature[];
}

export interface OpnsenseService {
	name: string;
	description: string | null;
	running: boolean;
}

/** A VPN tunnel, whatever its technology. */
export interface OpnsenseTunnelRow {
	/** `wireguard`, `openvpn` or `ipsec`. */
	kind: string;
	name: string;
	up: boolean | null;
	peers_total: number | null;
	peers_connected: number | null;
	/** Age of the last WireGuard handshake, in seconds. */
	last_handshake_age_seconds: number | null;
	bytes_in: number | null;
	bytes_out: number | null;
	detail: string | null;
	/** True when the tunnel is declared and carries no session. */
	down: boolean;
	/** True when the last WireGuard handshake is too old. */
	silent: boolean;
}

export interface OpnsenseCarpVip {
	interface: string | null;
	vhid: string | null;
	address: string | null;
	/** `MASTER`, `BACKUP`, `INIT` or `DISABLED`, as CARP says it. */
	status: string;
}

/** CARP state of a paired firewall. `null` on a standalone one. */
export interface OpnsenseCarp {
	enabled: boolean;
	/** Maintenance mode: switched over by hand and forgotten. */
	maintenance_mode: boolean;
	vips: OpnsenseCarpVip[];
}

export interface OpnsenseFirmware {
	/** False until the firewall has checked its mirror once: "no update" and "never looked" read the same in OPNsense's answer, so nothing below is meaningful then. */
	checked: boolean;
	version: string | null;
	latest: string | null;
	updates_pending: number | null;
	upgrade_available: boolean;
	reboot_required: boolean;
	last_check: string | null;
	connection_ok: boolean | null;
	/** OPNsense's own message, as is: it is the one that says *why*. */
	status_message: string | null;
}

/** The resolver. A firewall that routes but no longer resolves looks healthy to everyone but the machines behind it. */
export interface OpnsenseUnbound {
	running: boolean;
	queries: number | null;
	cache_hit_percent: number | null;
	blocklist_size: number | null;
}

export interface OpnsenseHealth {
	probed_at: number | null;
	version: string | null;
	os_version: string | null;
	product: string | null;
	hostname: string | null;
	system: OpnsenseSystem | null;
	services: OpnsenseService[];
	/** Stopped services, pulled out: this is what the page shows first. */
	stopped_services: OpnsenseService[];
	tunnels: OpnsenseTunnelRow[];
	/** Tunnels declared with no session. `0` when no VPN is configured. */
	tunnels_down: number;
	carp: OpnsenseCarp | null;
	firmware: OpnsenseFirmware | null;
	unbound: OpnsenseUnbound | null;
}

// ---------------------------------------------------------------------------
// TrueNAS SCALE / Community Edition (`crates/server/src/api/truenas.rs`,
// `crates/collectors/src/truenas/view.rs`). Dates are Unix seconds.
// ---------------------------------------------------------------------------

/** The scrub or resilver in progress, or the one that just finished. */
export interface TruenasScan {
	/** `SCRUB` or `RESILVER`. */
	function: string;
	/** `SCANNING`, `FINISHED`, `CANCELED`. */
	state: string;
	percent: number | null;
	errors: number | null;
	started_at: number | null;
	ended_at: number | null;
	seconds_left: number | null;
}

/** A device that is not `ONLINE`: the disk that failed inside a vdev still serving data. */
export interface TruenasDevice {
	/** The disk (`sdc`); failing that, the vdev's name. */
	name: string;
	status: string;
	/** `data`, `log`, `cache`, `spare`, `special`, `dedup`. */
	role: string;
}

/** A top-level vdev, to say the shape of the pool. */
export interface TruenasVdev {
	name: string;
	/** `MIRROR`, `RAIDZ1`, `DISK`… */
	kind: string;
	status: string;
	role: string;
	/** Number of disks in the vdev. */
	disks: number;
}

/**
 * A ZFS pool. The server flattens its `PoolView` into the row, beside the
 * `full` flag.
 */
export interface TruenasPoolRow {
	name: string;
	/** `ONLINE`, `DEGRADED`, `FAULTED`, `OFFLINE`, `UNAVAIL`, `SUSPENDED`… */
	status: string;
	/** ZFS's own verdict. A `DEGRADED` pool still serves its data. */
	healthy: boolean;
	/** Nothing broken, but look: resilver, features not enabled. */
	warning: boolean;
	/** The `zpool status` sentence, as is. */
	status_detail: string | null;
	size_bytes: number | null;
	allocated_bytes: number | null;
	free_bytes: number | null;
	used_percent: number | null;
	fragmentation_percent: number | null;
	scan: TruenasScan | null;
	/** End of the last completed scrub; absent after a resilver. */
	last_scrub_at: number | null;
	last_scrub_errors: number | null;
	scrub_threshold_days: number | null;
	/** Errors summed over the pool's disks since the last `zpool clear`. */
	read_errors: number;
	write_errors: number;
	checksum_errors: number;
	unhealthy_devices: TruenasDevice[];
	vdevs: TruenasVdev[];
	/** True past 80 % used. */
	full: boolean;
}

/** A dataset, flattened from `DatasetView` beside `near_quota`. */
export interface TruenasDatasetRow {
	name: string;
	pool: string | null;
	used_bytes: number | null;
	available_bytes: number | null;
	/** `quota`, or else `refquota`. Absent when none is set — not a quota of zero. */
	quota_bytes: number | null;
	quota_used_percent: number | null;
	snapshot_count: number | null;
	/** When a periodic snapshot task covering this dataset (itself or a recursive parent) last took one. Null when no task covers it. */
	newest_snapshot_at: number | null;
	encrypted: boolean;
	/** Encrypted with its key not loaded: the data is there but unreadable. */
	locked: boolean;
	/** True past 90 % of the quota. */
	near_quota: boolean;
}

/** A disk, flattened from `DiskView` beside `hot`. */
export interface TruenasDiskRow {
	/** `sda`, `nvme0n1`. */
	name: string;
	serial: string | null;
	model: string | null;
	/** `HDD` or `SSD`. */
	kind: string | null;
	size_bytes: number | null;
	pool: string | null;
	temperature_celsius: number | null;
	/** `SUCCESS`, `RUNNING`, `ABORTED`, `FAILED`; absent when no test ever ran. */
	smart_last_status: string | null;
	smart_last_test: string | null;
	/** True when any test in the log failed. */
	smart_failed: boolean;
	/** True at 50 °C and above. */
	hot: boolean;
}

export interface TruenasStorage {
	probed_at: number | null;
	/** Unhealthy pools first. */
	pools: TruenasPoolRow[];
	unhealthy_pools: number;
	datasets: TruenasDatasetRow[];
	snapshots_total: number | null;
	/** SMART failures first, then hot disks, then by name. */
	disks: TruenasDiskRow[];
}

/** One row per pool: the last scrub and what it found. */
export interface TruenasScrubRow {
	pool: string;
	last_scrub_at: number | null;
	last_scrub_errors: number | null;
	threshold_days: number;
	overdue: boolean;
	/** True during a scrub or a resilver. */
	running: boolean;
	/** `SCRUB` or `RESILVER` when a scan is running or just finished. */
	function: string | null;
	percent: number | null;
	seconds_left: number | null;
}

/** A replication or periodic snapshot task, flattened from `TaskView`. */
export interface TruenasTaskRow {
	/** `replication` or `snapshot`. */
	kind: string;
	/** The replication's name, or the snapshot task's dataset. */
	name: string;
	enabled: boolean;
	/** `PENDING`, `WAITING`, `RUNNING`, `FINISHED`, `ERROR`, `HOLD`. */
	state: string;
	last_run_at: number | null;
	last_snapshot: string | null;
	/** The error sentence, or the reason for a hold. */
	error: string | null;
	/** A short detail: `PUSH over SSH`, `keep 2 WEEK`. */
	detail: string | null;
	failed: boolean;
	/** Enabled and not run for more than eight days. */
	stale: boolean;
}

export interface TruenasProtection {
	probed_at: number | null;
	scrubs: TruenasScrubRow[];
	/** Replications then snapshots, failures first. */
	tasks: TruenasTaskRow[];
	failed_tasks: number;
}

export interface TruenasSystem {
	uptime_seconds: number | null;
	load: number[];
	memory_total_bytes: number | null;
	cpu_count: number | null;
	cpu_model: string | null;
	product: string | null;
	ecc_memory: boolean | null;
}

/** An alert TrueNAS raised itself, not dismissed. */
export interface TruenasAlert {
	/** `VolumeStatus`, `SMART`, `ZpoolCapacityWarning`… */
	klass: string;
	/** `INFO` to `EMERGENCY`. */
	level: string;
	message: string;
	raised_at: number | null;
}

export interface TruenasAlertCount {
	level: string;
	count: number;
}

/** A service that starts with the NAS. */
export interface TruenasService {
	name: string;
	/** False for `STOPPED`, and for `UNKNOWN` when the service's probe timed out. */
	running: boolean;
	state: string;
}

export interface TruenasHealth {
	probed_at: number | null;
	version: string | null;
	hostname: string | null;
	system: TruenasSystem | null;
	/** Active alerts, most severe first. */
	alerts: TruenasAlert[];
	/** Alerts per level, `INFO` to `EMERGENCY`, zeros included. */
	alert_counts: TruenasAlertCount[];
	services: TruenasService[];
	stopped_services: TruenasService[];
}
