import type { ChannelKindInfo } from './notify.js';
import type { TargetId } from './targets.js';

// --- Server logs (Settings → System → Logs) -----------------------------------

export type LogLevel = 'error' | 'warn' | 'info' | 'debug';

export interface LogFile {
	/** Day, `YYYY-MM-DD`. */
	day: string;
	bytes: number;
	compressed: boolean;
}

/** Local log files and the disk writer. Mirrors `GET /api/logs`. */
export interface LogsInfo {
	directory: string;
	retention_days: number;
	max_total_mb: number;
	total_bytes: number;
	oldest_day: string | null;
	days_available: number;
	files: LogFile[];
	writer: { active: boolean; dropped: number };
}

export interface LogsSettingsUpdate {
	retention_days?: number;
	max_total_mb?: number;
}

export interface LogExportEstimate {
	days: number;
	from: string;
	to: string;
	files: number;
	/** Estimated size of the downloaded .tar.gz. */
	bytes: number;
}

/** Same shape as a channel kind, without delivery actions. */
export type LogKindInfo = Omit<ChannelKindInfo, 'actions'>;

export interface LogDestinationStats {
	sent: number;
	dropped: number;
	failed_batches: number;
	queued: number;
	last_error: string | null;
	last_error_at: string | null;
	last_success_at: string | null;
}

/** A destination as returned on read: never its secrets. */
export interface LogDestination {
	id: number;
	name: string;
	kind: string;
	enabled: boolean;
	min_level: LogLevel;
	settings: Record<string, unknown>;
	has_secret: boolean;
	stats: LogDestinationStats | null;
}

export interface LogDestinationPayload {
	name: string;
	kind: string;
	enabled: boolean;
	min_level: LogLevel;
	settings: Record<string, unknown>;
	/** Omitted = keep the stored secrets, `{}` = clear them. */
	secrets?: Record<string, unknown>;
}

export interface LogTestResult {
	ok: boolean;
	message: string;
}

// --- CVE & vulnerabilities (mirrors `crates/server/src/api/cves.rs`) --------

export type CveState = 'vulnerable' | 'patched' | 'unknown';
export type CveSeverity = 'critical' | 'high' | 'medium' | 'low' | 'none';
export type CveReason =
	| 'below_fixed_version'
	| 'in_affected_range'
	| 'no_fix_available'
	| 'fixed_version_installed'
	| 'vendor_build'
	| 'undetermined'
	| 'version_unparsed';

/** `GET`/`PUT /api/security/cves/settings`. The NVD key never comes back. */
export interface CveSettings {
	enabled: boolean;
	has_nvd_key: boolean;
}

/** Body of `PUT /api/security/cves/settings`: `nvd_api_key` omitted keeps the key, `""` clears it. */
export interface CveSettingsInput {
	enabled: boolean;
	nvd_api_key?: string;
}

/** Body of `POST /api/security/cves/targets`: the CVE check of several devices (1 to 500 ids). */
export interface CveTargetsInput {
	target_ids: TargetId[];
	enabled: boolean;
}

/** `updated`: devices whose setting really changed (unknown ids and those already so do not count). */
export interface CveTargetsResult {
	updated: number;
}

export interface CveSource {
	name: 'nvd' | 'osv' | 'debian' | 'kev';
	last_success: string | null;
	last_error: string | null;
	items: number;
}

export interface CveCounts {
	vulnerable: number;
	exploited: number;
	unknown: number;
	patched: number;
	ignored: number;
}

export interface CveSummary {
	enabled: boolean;
	refreshing: boolean;
	last_refresh: string | null;
	counts: CveCounts;
	targets: number;
	components: number;
	sources: CveSource[];
}

export interface CveFinding {
	target_id: number;
	target_name: string;
	target_kind: string;
	component: string;
	installed_version: string;
	fixed_version: string | null;
	cve_id: string;
	summary: string;
	cvss: number | null;
	severity: CveSeverity;
	published: string | null;
	exploited: boolean;
	ransomware: boolean;
	state: CveState;
	reason: CveReason;
	source: 'nvd' | 'osv' | 'debian';
	link: string;
	ignored: boolean;
	ignore_note: string | null;
	ignored_by: string | null;
	ignored_at: string | null;
}

export interface CveList {
	enabled: boolean;
	findings: CveFinding[];
}

export interface CveComponent {
	target_id: number;
	target_name: string;
	component: string;
	version: string;
	origin: string;
	findings: number;
}

export interface CveInventory {
	components: CveComponent[];
}

/** Query of `GET /api/security/cves`. */
export interface CveFilters {
	target?: number;
	severity?: CveSeverity;
	state?: CveState;
	exploited?: boolean;
	ignored?: boolean;
}
