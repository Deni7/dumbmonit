/**
 * `GET /api/overview/widgets?range=24h`: the extra figures of the overview
 * dashboards (A, B, C). Everything else on those pages comes from the existing
 * stores and routes (alerts, silences, SLOs, deadlines, status pages).
 * Mirrors `crates/server/src/api/overview_widgets/` (Rust). Timestamps are UTC
 * without suffix, like everywhere else.
 */
import type { TargetId } from './targets.js';

/** Monitor family, used for the site × category views. */
export type WidgetCategory = 'servers' | 'storage' | 'network' | 'services' | 'backups' | 'other';
/** Overview status of a monitor (same words as the console). */
export type WidgetStatus = 'up' | 'down' | 'degraded' | 'paused';

export interface WidgetMonitor {
	id: TargetId;
	name: string;
	/** Short kind label, e.g. "Proxmox VE node", "HTTPS check". */
	kind: string;
	category: WidgetCategory;
	status: WidgetStatus;
}

/** A site: the targets reached through one relay, or "this site" for the rest. */
export interface WidgetSite {
	/** Stable key: `local` or `relay:<name>`. */
	key: string;
	name: string;
	monitors: WidgetMonitor[];
	/** Latency to the relay (ms) over the range, oldest first; null for `local`. */
	wan_latency_ms: number[] | null;
	/** Next maintenance window covering a monitor of this site, if any. */
	next_maintenance: string | null;
	/** End of the last successful backup job of this site, if known. */
	last_backup: string | null;
}

export interface TopEntry {
	target_id: TargetId;
	/** Monitor name, or "<monitor> <volume>" for disks and interfaces. */
	label: string;
	site: string;
	value: number;
	/** `%` for cpu/mem/disk, `Mb/s` for net. */
	unit: string;
	/** Values over the range, oldest first (≤ 48 points). */
	series: number[];
	/** Disk only: days before full at the current growth, if computable. */
	forecast_days?: number | null;
}

export interface TopConsumers {
	cpu: TopEntry[];
	mem: TopEntry[];
	disk: TopEntry[];
	net: TopEntry[];
}

export interface BackupRun {
	at: string;
	ok: boolean;
}

export interface BackupJob {
	/** Job or device name as the collector reports it. */
	job: string;
	/** "PBS", "Active Backup", "Hyper Backup", "restic", "borg", "Veeam"… */
	engine: string;
	/** What is backed up where, one line. */
	detail: string;
	site: string;
	target_id: TargetId;
	last_success: string | null;
	duration_s: number | null;
	changed_bytes: number | null;
	/** Last runs, oldest first (≤ 14). Empty when the source gives no history. */
	runs: BackupRun[];
	status: 'ok' | 'failed' | 'running' | 'stale';
}

export interface ServiceCheck {
	target_id: TargetId;
	name: string;
	/** "HTTPS", "HTTP", "DNS", "TLS", "SMTP", "Ping", "TCP"… */
	kind: string;
	status: WidgetStatus;
	/** Last checks, oldest first (≤ 36): true = success, false = failure, null = no data. */
	checks: (boolean | null)[];
	p95_ms: number | null;
	availability_30d: number | null;
}

export interface AlertDay {
	/** `YYYY-MM-DD` (UTC). */
	day: string;
	critical: number;
	warning: number;
	info: number;
}

export interface AlertStats {
	/** Last 30 days, oldest first. */
	days: AlertDay[];
	total: number;
	previous_total: number;
	mtta_s: number | null;
	previous_mtta_s: number | null;
	mttr_s: number | null;
	previous_mttr_s: number | null;
	/** Share of alerts acknowledged within 15 minutes, 0–100. */
	acked_within_15m_pct: number | null;
	noisiest: { rule: string; target: string; target_id: TargetId | null; count: number } | null;
}

export interface AgentVersionCount {
	version: string;
	count: number;
	latest: boolean;
}

export interface AgentSummary {
	total: number;
	versions: AgentVersionCount[];
	relays: { name: string; connected: boolean; last_seen: string | null }[];
	server_update: { version: string } | null;
}

export interface WidgetEvent {
	at: string;
	kind: 'crit' | 'warn' | 'ok' | 'info' | 'maint' | 'ack';
	text: string;
	target_id: TargetId | null;
}

export interface AvailabilitySeries {
	/** Last 30 days, oldest first; pct null when no data that day. */
	days: { day: string; pct: number | null }[];
	avg_30d: number | null;
	previous_avg_30d: number | null;
	/** Instance-wide availability objective (%), default 99.9. */
	slo_target: number;
	/** Error budget left over 30 days against `slo_target`, 0–100. */
	budget_left_pct: number | null;
}

export interface AlertsOpened {
	/** Last 14 days, oldest first. */
	days: { day: string; count: number }[];
	this_week: number;
	last_week: number;
	/** When the last alert was resolved, if any. */
	last_resolved_at: string | null;
}

export interface OverviewWidgets {
	sites: WidgetSite[];
	top: TopConsumers;
	backups: BackupJob[];
	services: ServiceCheck[];
	alert_stats: AlertStats;
	agents: AgentSummary;
	events: WidgetEvent[];
	availability: AvailabilitySeries;
	alerts_opened: AlertsOpened;
}

