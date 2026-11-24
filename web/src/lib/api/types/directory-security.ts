import type { TargetId } from './targets.js';

// --- Active Directory (crates/server/src/api/activedirectory.rs) -------------

/** The security score's scale; the interface shows it as info, advisory, warning. */
export type AdFindingSeverity = 'low' | 'medium' | 'high' | 'critical';

/** A raw security fact: a stable id, how many objects, and a few of them. */
export interface AdFinding {
	id: string;
	severity: AdFindingSeverity;
	/** `privileged`, `kerberos`, `accounts`, `policy`, `hygiene`, `infrastructure`. */
	category: string;
	title: string;
	detail: string;
	/** Objects concerned; 0: examined, nothing found. */
	count: number;
	/** Up to twenty account names, sorted. */
	samples: string[];
}

export interface AdFindingCounts {
	low: number;
	medium: number;
	high: number;
	critical: number;
}

export interface AdConnection {
	host: string;
	port: number;
	/** `ldaps`, `starttls` or `plain`. */
	security: string;
	certificate_verified: boolean;
	connect_seconds: number | null;
	bind_seconds: number | null;
}

export interface AdDomain {
	dns_name: string;
	naming_context: string;
	forest_dns_name: string;
	forest_root: boolean;
	domain_sid: string | null;
	domain_level: number | null;
	domain_level_label: string | null;
	forest_level: number | null;
	forest_level_label: string | null;
	dc_host_name: string | null;
	dc_synchronized: boolean | null;
	dc_global_catalog_ready: boolean | null;
	/** Domain controller clock minus DumbMonit's, in seconds. */
	clock_skew_seconds: number | null;
	recycle_bin_enabled: boolean | null;
}

export interface AdPolicy {
	min_password_length: number | null;
	password_history_length: number | null;
	/** `null`: passwords never expire. */
	max_password_age_seconds: number | null;
	min_password_age_seconds: number | null;
	complexity_required: boolean | null;
	/** `0`: no lockout. */
	lockout_threshold: number | null;
	/** `null`: locked until unlocked by hand. */
	lockout_duration_seconds: number | null;
	lockout_window_seconds: number | null;
	machine_account_quota: number | null;
}

export interface AdDomainController {
	name: string;
	host_name: string | null;
	site: string;
	global_catalog: boolean;
	read_only: boolean;
	operating_system: string | null;
	os_version: string | null;
	queried: boolean;
	/** `reachable`, `unreachable`, `unresolved` or `not_checked`. */
	reachability: string;
	reachability_detail: string | null;
	inbound_connections: number;
	/** FSMO roles held: `schema`, `domain_naming`, `pdc`, `rid`, `infrastructure`. */
	roles: string[];
}

export interface AdFsmo {
	role: string;
	label: string;
	holder: string | null;
}

export interface AdNeighbor {
	source: string;
	naming_context: string;
	last_success: number | null;
	last_attempt: number | null;
	last_result: number;
	consecutive_failures: number;
}

export interface AdReplication {
	/** `readable`, `not_readable` or `single_dc`. */
	status: string;
	neighbors: AdNeighbor[];
	failing: number;
}

export interface AdMember {
	name: string;
	kind: 'user' | 'computer' | 'group' | 'other';
	enabled: boolean;
}

export interface AdPrivilegedGroup {
	name: string;
	rid: number;
	/** `false`: not in this domain (Enterprise and Schema Admins live in the forest root). */
	found: boolean;
	dn: string | null;
	member_count: number;
	enabled_member_count: number;
	nested_group_count: number;
	members: AdMember[];
	truncated: boolean;
}

export interface AdInventory {
	refreshed_at: number;
	users_total: number;
	users_enabled: number;
	users_disabled: number;
	users_locked_out: number;
	users_password_never_expires: number;
	users_stale: number;
	computers_total: number;
	computers_enabled: number;
	computers_stale: number;
	groups_total: number;
	admin_count_accounts: number;
	krbtgt_password_last_set: number | null;
	krbtgt_password_age_seconds: number | null;
	laps_schema: boolean;
	laps_computers: number;
	laps_eligible_computers: number;
	stale_days_users: number;
	stale_days_computers: number;
}

/** `GET /targets/{id}/ad`. Dates are Unix seconds. */
export interface AdOverview {
	probed_at: number | null;
	connection: AdConnection | null;
	/** Why the service account was refused; everything else is then empty. */
	bind_error: string | null;
	domain: AdDomain | null;
	policy: AdPolicy | null;
	dcs: AdDomainController[];
	fsmo: AdFsmo[];
	replication: AdReplication | null;
	privileged_groups: AdPrivilegedGroup[];
	/** `null` until the first background inventory completes. */
	inventory: AdInventory | null;
	findings: AdFinding[];
	finding_counts: AdFindingCounts;
	errors: string[];
}

/** `GET /targets/{id}/ad/findings`. */
export interface AdFindings {
	probed_at: number | null;
	inventory_at: number | null;
	findings: AdFinding[];
	counts: AdFindingCounts;
}

// --- Security score (crates/server/src/api/security.rs) -----------------------

export type SecurityCategory =
	| 'exposure'
	| 'patching'
	| 'authentication'
	| 'encryption'
	| 'backup'
	| 'configuration';

export type SecuritySeverity = 'critical' | 'high' | 'medium' | 'low';

export type SecurityResult = 'pass' | 'fail' | 'unknown';

export type SecurityGrade = 'A' | 'B' | 'C' | 'D' | 'F';

/** One PingCastle-style check. Mirrors `SecurityCheckView`. */
export interface SecurityCheck {
	id: string;
	title: string;
	category: SecurityCategory;
	severity: SecuritySeverity;
	/** critical 10, high 6, medium 3, low 1. */
	weight: number;
	result: SecurityResult;
	evidence: string;
	remediation: string;
	/** Vendor best-practice URL. */
	reference: string;
}

export interface SecurityCounts {
	pass: number;
	fail: number;
	unknown: number;
}

/** How to raise the score on this kind of device. Mirrors `GuideView`. */
export interface SecurityGuide {
	/** Product name, e.g. "Proxmox VE", "Linux host". */
	product: string;
	/** Concrete steps, in the order to take them (English, server-provided). */
	steps: string[];
	/** The matching section of the "Security score" documentation page. */
	doc_url: string;
}

/**
 * `GET /api/targets/{id}/security`: the security score of one device.
 * Mirrors `SecurityReport`. 404 only when the device itself does not exist;
 * a kind with no checks answers 200 with `supported: false`.
 */
export interface SecurityReport {
	/** `false`: the security score is turned off in Settings; nothing else is filled. */
	enabled: boolean;
	target_id: TargetId;
	target_name: string;
	kind: string;
	/** `false`: this kind has no security checks; `checks` is empty, `score`/`grade` are `null`. */
	supported: boolean;
	/** 0-100, or `null` when every check is `unknown`. */
	score: number | null;
	grade: SecurityGrade | null;
	/** A failing critical check capped the grade at C or worse. */
	capped: boolean;
	/** Server timestamp, UTC without suffix: parse with `parseServerDate`. */
	evaluated_at: string;
	counts: SecurityCounts;
	/** Sorted: fail (heaviest first), then pass, then unknown. */
	checks: SecurityCheck[];
	/** `null` when `supported` is false. */
	guide: SecurityGuide | null;
}

/** One row of `GET /api/security/summary`. Mirrors `SecuritySummaryDevice`. */
export interface SecuritySummaryDevice {
	target_id: TargetId;
	target_name: string;
	kind: string;
	score: number | null;
	grade: SecurityGrade | null;
	capped: boolean;
	pass: number;
	fail: number;
	unknown: number;
	/** Up to 3 failing titles, heaviest first. */
	top_failures: string[];
}

/**
 * `GET /api/security/summary`: every device whose kind has security checks,
 * worst first. Mirrors `SecuritySummary`.
 */
export interface SecuritySummary {
	/** `false`: the security score is turned off in Settings; `devices` is empty. */
	enabled: boolean;
	evaluated_at: string;
	devices: SecuritySummaryDevice[];
}

/** `GET`/`PUT /api/security/settings`: the opt-in switch (off by default). */
export interface SecuritySettings {
	enabled: boolean;
}

/** Where the threat map's counts come from. Mirrors `threats::Source`. */
export type ThreatSourceKind = 'crowdsec' | 'firewall' | 'cloudflare';

/** One country of `GET /api/security/threats`. Mirrors `ThreatCountry`. */
export interface ThreatCountry {
	/** ISO 3166-1 alpha-2. */
	code: string;
	total: number;
	crowdsec: number;
	firewall: number;
	cloudflare: number;
}

/** A source plugged into the threat map, even with nothing seen. Mirrors `ThreatSource`. */
export interface ThreatSource {
	source: ThreatSourceKind;
	/** Devices feeding it. */
	targets: TargetId[];
	total: number;
	/** Time span its counts cover. */
	window_secs: number;
}

/**
 * `GET /api/security/threats`: attacks stopped, by country of origin, every
 * source together. Countries only, never an address. Mirrors `ThreatMap`.
 */
export interface ThreatMap {
	/** Busiest first. */
	countries: ThreatCountry[];
	sources: ThreatSource[];
	/** VictoriaMetrics did not answer: only the firewall live windows are counted. */
	partial: boolean;
	evaluated_at: string;
}

/** One of the three conditions of a heartbeat. Mirrors `selfmon::Component`. */
export interface SelfMonitoringComponent {
	name: 'scheduler' | 'database' | 'victoria';
	ok: boolean;
	error?: string;
}

/** The last heartbeat attempted. Mirrors `selfmon::PingRecord`. */
export interface HeartbeatPing {
	/** UTC without suffix. */
	at: string;
	/** `withheld`: a component was down and nothing was sent. */
	outcome: 'up' | 'fail' | 'withheld';
	/** The call left and the service accepted it. */
	ok: boolean;
	error?: string;
	failed: string[];
}

/** The outgoing heartbeat setting. The URL itself never comes back. Mirrors `HeartbeatView`. */
export interface Heartbeat {
	enabled: boolean;
	interval_seconds: number;
	send_fail: boolean;
	has_url: boolean;
	url_host: string | null;
	last_ping: HeartbeatPing | null;
}

/** `PUT /api/self-monitoring/heartbeat`. `url` omitted keeps it, `''` clears it. */
export interface HeartbeatUpdate {
	enabled: boolean;
	interval_seconds?: number;
	send_fail?: boolean;
	url?: string;
}

/** `POST /api/self-monitoring/heartbeat/test`. */
export interface HeartbeatTestResult {
	ok: boolean;
	error: string | null;
}

/** `GET /api/self-monitoring`: health of DumbMonit itself. Mirrors `Overview`. */
export interface SelfMonitoring {
	status: 'ok' | 'degraded';
	components: SelfMonitoringComponent[];
	data_free_bytes: number | null;
	data_total_bytes: number | null;
	database_bytes: number;
	vm_embedded: boolean;
	vm_restarts: number;
	scheduler_idle_seconds: number;
	scheduler_backlog: number;
	heartbeat: Heartbeat;
}

/**
 * A browser subscribed to Web Push for the signed-in account. Mirrors
 * `notify::webpush::store::Device`. Neither the full endpoint nor the
 * browser keys are ever returned.
 */
export interface PushDevice {
	id: number;
	/** Readable label derived from the User-Agent: "Firefox on Android". */
	device: string;
	/** Host of the browser's push service (`fcm.googleapis.com`, `web.push.apple.com`…). */
	push_service: string;
	/** First 16 hex characters of SHA-256(endpoint): the browser recognises itself with it. */
	fingerprint: string;
	created_at: string;
	last_success_at: string | null;
	last_error: string | null;
}

/** `GET /api/webpush`. Mirrors `api::webpush::Overview`. */
export interface PushOverview {
	/** VAPID public key, base64url: the `applicationServerKey` to subscribe with. */
	public_key: string;
	devices: PushDevice[];
}

/** `POST /api/webpush/test`. Mirrors `api::webpush::TestReport`. */
export interface PushTestReport {
	ok: boolean;
	delivered: number;
	failed: number;
	removed: number;
	message: string;
}
