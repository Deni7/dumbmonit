// --- Data & retention (Settings → System) and log search -------------------
// Mirrors `GET/PUT /api/system/storage` and `GET /api/logs/search`.

export type StorageUsageKey = 'metrics' | 'rollup' | 'logs_store' | 'database' | 'logs' | 'backups';
export type SettingSource = 'settings' | 'environment' | 'default';

export interface StorageView {
	metrics: {
		/** False: external VictoriaMetrics, its retention is managed there. */
		embedded: boolean;
		retention_days: number;
		retention_source: SettingSource;
		running: boolean;
	};
	/** Long-term 5-minute aggregates, in a second embedded VictoriaMetrics. */
	rollup: {
		available: boolean;
		embedded: boolean;
		enabled: boolean;
		running: boolean;
		retention_days: number;
		step_seconds: number;
		/** UTC without suffix: end of the latest aggregated window. */
		last_window: string | null;
		/** Oldest aggregated window so far. */
		backfilled_until: string | null;
		backfill_done: boolean;
		series: number | null;
		last_error: string | null;
	};
	/** Embedded VictoriaLogs, off by default. */
	logs_store: {
		available: boolean;
		enabled: boolean;
		running: boolean;
		retention_days: number;
		last_error: string | null;
	};
	disk: {
		usage: { key: StorageUsageKey; path: string; bytes: number | null }[];
		free_bytes: number | null;
		total_bytes: number | null;
	};
	growth: {
		bytes_per_day: number | null;
		days_until_full: number | null;
		sampled_days: number;
	};
	limits: {
		min_retention_days: number;
		max_retention_days: number;
		min_rollup_retention_days: number;
		max_rollup_retention_days: number;
		min_logs_retention_days: number;
		max_logs_retention_days: number;
	};
	/** Empty on GET; on PUT, the processes (re)started or stopped. */
	restarted: ('metrics' | 'rollup' | 'logs_store')[];
}

export interface StorageUpdate {
	metrics_retention_days?: number;
	rollup_enabled?: boolean;
	rollup_retention_days?: number;
	logs_store_enabled?: boolean;
	logs_store_retention_days?: number;
}

export interface LogSearchEntry {
	/** UTC without suffix. */
	time: string;
	level: string;
	message: string;
	/** Stream: `dumbmonit` (the server), `agent`, `syslog` or `trap`. */
	source: string;
	host: string | null;
	/** Unit, file, channel or sender address. */
	origin: string | null;
	/** Device the sender is attached to, when its address matches one. */
	target_name: string | null;
	fields: Record<string, string>;
}

export interface LogSearchResult {
	entries: LogSearchEntry[];
	truncated: boolean;
}

export interface LogSearchParams {
	query?: string;
	/** Milliseconds since the epoch. */
	start?: number;
	end?: number;
	level?: 'error' | 'warn' | 'info' | 'debug';
	limit?: number;
	source?: LogSource;
	/** Device id: its agent, syslog and trap lines only. */
	target?: number;
}

export type LogSource = 'server' | 'agent' | 'syslog' | 'trap' | 'all';

/** Log collection of one agent (`/api/logs/agents/{id}`). */
export interface AgentLogsSetting {
	enabled: boolean;
	min_level: 'error' | 'warn' | 'info' | 'debug';
	journald: boolean;
	/** Followed files, absolute paths with `*` and `?` wildcards. */
	files: string[];
	/** Windows event log channels. */
	eventlog: string[];
}

export interface AgentLogs extends AgentLogsSetting {
	/** UTC without suffix. */
	last_received_at: string | null;
	received_total: number;
	/** False when the embedded log store is off: received lines are lost. */
	store_running: boolean;
}

/** State of one receiver (syslog or SNMP traps). */
export interface LogInput {
	enabled: boolean;
	listening: string[];
	received: number;
	rejected: number;
	/** Messages from a sender no device carries. */
	unmatched: number;
	last_at: string | null;
	error: string | null;
}

export interface LogInputs {
	store_running: boolean;
	syslog: LogInput;
	traps: LogInput;
	written: number;
	dropped: number;
}

// --- Metrics explorer -----------------------------------------------------
// Mirrors `crates/server/src/api/metrics.rs` (catalog).

export interface CatalogLabel {
	name: string;
	values: string[];
	more: boolean;
}

export interface CatalogMetric {
	name: string;
	series: number;
	labels: CatalogLabel[];
}

export interface MetricsCatalog {
	metrics: CatalogMetric[];
	truncated: boolean;
}
