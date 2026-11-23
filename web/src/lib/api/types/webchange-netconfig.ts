// --- Website changes (webchange) --------------------------------------------

/**
 * One watched page of a `webchange` device, as last probed.
 *
 * `error` is set when the last fetch of this page failed (page unreachable,
 * removed…); `status` is the HTTP status of that attempt, or `null` before
 * the first check. `latest_snapshot` is the id of its most recent stored text,
 * or `null` when none was ever taken.
 */
export interface WebchangePage {
	url: string;
	title: string | null;
	status: number | null;
	error: string | null;
	last_checked: string | null;
	last_changed: string | null;
	changes: number;
	latest_snapshot: number | null;
}

/** `GET /api/targets/{id}/webchange/pages`. */
export interface WebchangePagesResponse {
	/** Pages found when Max pages stopped the last crawl (read and still queued); `null` when the whole site was read. */
	found: number | null;
	pages: WebchangePage[];
}

export type WebchangeChangeKind = 'changed' | 'new_page' | 'removed_page';

/** One detected change, newest first; mirrors `GET .../webchange/changes`. */
export interface WebchangeChange {
	id: number;
	url: string;
	kind: WebchangeChangeKind;
	detected_at: string;
	added: number;
	removed: number;
	/** Id of the snapshot taken before the change, or `null` for a new page. */
	before: number | null;
	/** Id of the snapshot taken after the change, or `null` for a removed page. */
	after: number | null;
	/** Same value for the same lines removed and added on several pages; `null` for a new or removed page. */
	signature: string | null;
}

/** One fetched copy of a page, referenced by a change's `before`/`after`. */
export interface WebchangeSnapshot {
	id: number;
	fetched_at: string;
	title: string | null;
}

export type WebchangeDiffOp = 'equal' | 'insert' | 'delete' | 'skip';

/** One line of the text diff; `skip` collapses a run of `count` unchanged lines. */
export interface WebchangeDiffLine {
	op: WebchangeDiffOp;
	text: string;
	count: number | null;
}

/** `GET .../webchange/changes/{change_id}`: the change plus what it takes to show it. */
export interface WebchangeChangeDetail extends WebchangeChange {
	before_snapshot: WebchangeSnapshot | null;
	after_snapshot: WebchangeSnapshot | null;
	diff: WebchangeDiffLine[];
}

// --- Network configuration backups (netconfig) ------------------------------

/** One stored configuration version (`GET /targets/{id}/netconfig`), without its text. */
export interface NetconfigVersion {
	id: number;
	fetched_at: string;
	/** Last backup that found this same configuration. */
	last_seen_at: string;
	profile: string;
	size_bytes: number;
	lines: number;
	added: number;
	removed: number;
}

export interface NetconfigHostKey {
	/** Remembered SHA-256 fingerprint (`SHA256:…`); null before the first SSH backup. */
	pinned: string | null;
	pinned_at: string | null;
	/** A different key the device presented and that was refused, waiting for an administrator. */
	changed_to: string | null;
	changed_at: string | null;
}

export type NetconfigErrorKind = 'down' | 'auth' | 'config' | 'host_key';

/** `GET /targets/{id}/netconfig`. */
export interface NetconfigOverview {
	profile: string | null;
	profile_label: string | null;
	transport: 'ssh_shell' | 'ssh_exec' | 'http' | null;
	running: boolean;
	last_attempt_at: string | null;
	last_success_at: string | null;
	last_error: string | null;
	last_error_kind: NetconfigErrorKind | null;
	host_key: NetconfigHostKey;
	/** The viewer may read version contents, secrets masked (operator or admin). */
	can_read: boolean;
	/** The viewer may reveal secrets and download (admin signed in). */
	can_reveal: boolean;
	versions: NetconfigVersion[];
}

/** `GET /targets/{id}/netconfig/versions/{version_id}`. */
export interface NetconfigVersionContent {
	version: NetconfigVersion;
	masked: boolean;
	masked_values: number;
	content: string;
}

/** `GET /targets/{id}/netconfig/diff`: same line format as website changes. */
export interface NetconfigCompare {
	from: NetconfigVersion | null;
	to: NetconfigVersion;
	masked: boolean;
	added: number;
	removed: number;
	diff: WebchangeDiffLine[];
}

// --- Metrics ----------------------------------------------------------------

/** A series in Prometheus format: `[timestamp_seconds, "value"]`. */
export interface MetricSeries {
	metric: Record<string, string>;
	values: [number, string][];
}
