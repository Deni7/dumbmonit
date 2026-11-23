import type { AfterChange } from './agents.js';
import type { RuleOverride } from './notify.js';
import type { TargetId } from './targets.js';

// --- Alerts -----------------------------------------------------------------

/** Severity ladder as declared by a rule (mapped to the weather words in the UI). */
export type AlertSeverity = 'info' | 'warning' | 'critical';

/** Where the state machine stands for one alert. */
export type AlertPhase = 'ok' | 'pending' | 'firing' | 'resolved';

/** What the user must read: `phase`, but with suppression made visible. */
export type AlertEffectivePhase = AlertPhase | 'suppressed';

/**
 * How a rule watches its metric. `forecast`: days before full (capacity
 * families or a custom expression) against `threshold`; `drift`: how many
 * times the usual value the measurement is, against `threshold`.
 */
export type RuleKind = 'threshold' | 'anomaly' | 'predict' | 'log_pattern' | 'forecast' | 'drift' | 'slo_burn';

/** Direction a `drift` rule watches. */
export type DriftDirection = 'up' | 'down' | 'both';

/** Comparison operator of a threshold rule. */
export type RuleOperator = '>' | '>=' | '<' | '<=';

/** Scope of a rule: everything, an explicit list, or a tag match. */
export type TargetSelector =
	| { kind: 'all' }
	| { kind: 'ids'; ids: TargetId[] }
	| { kind: 'labels'; labels: Record<string, string> };

/** Anomaly-detection settings (`anomaly` rules) and log-pattern settings (`log_pattern` rules). */
export interface AnomalyParams {
	k: number;
	alpha: number;
	mad_floor_abs: number;
	mad_floor_rel: number;
	min_samples: number;
	/** `log_pattern`: counting window, in seconds (60…86400). */
	log_window_secs: number;
	/** `log_pattern`: one alert per device (true) or a single one. */
	log_per_target: boolean;
	/** `forecast`: days of history fitted (2…90, default 7). */
	forecast_window_days?: number;
	/** `forecast`: at least warning below this many days (default 14). */
	forecast_warning_days?: number;
	/** `forecast`: critical below this many days (default 7). */
	forecast_critical_days?: number;
	/** `forecast`: forecasts less sure than this (0…1) never fire. */
	forecast_min_confidence?: number;
	/** `forecast` on a custom expression: the value at which it is full. */
	forecast_limit?: number;
	/** `drift`: window of the current measurement, in seconds. */
	drift_window_secs?: number;
	/** `drift`: previous weeks compared at the same hour (0 = 7-day median only). */
	drift_weeks?: number;
	/** `drift`: minimum points in the window. */
	drift_min_samples?: number;
	/** `drift`: smaller values never count. */
	drift_min_value?: number;
	/** `drift`: minimum absolute gap to the usual value. */
	drift_min_delta?: number;
	drift_direction?: DriftDirection;
}

/**
 * One active alert, mirroring the server's `ActiveAlertView`.
 *
 * A `firing` alert can be `suppressed` (its parent device is unreachable): the
 * `phase` stays `firing` for the engine — so it is not re-notified when the
 * parent recovers — while `effective_phase` becomes `suppressed`, which is what
 * the interface displays. There is no `target_name` here: resolve it from the
 * targets list via `target_id`.
 */
/** A ticket a channel (Jira, GLPI, Zammad, GitHub, GitLab) opened for an alert. */
export interface AlertTicket {
	channel_id: number;
	channel_name: string;
	/** `jira`, `glpi`, `zammad`, `github_issues` or `gitlab_issues`. */
	kind: string;
	/** Reference shown to the user, like `OPS-12` or `#34`. */
	label: string;
	url: string;
	state: 'open' | 'closed';
}

export interface Alert {
	fingerprint: string;
	rule_uid: string;
	/** Current name of the rule; empty if the rule was deleted since. */
	rule_name: string;
	severity: AlertSeverity;
	target_id: TargetId | null;
	series_key: string;
	labels: Record<string, string>;
	phase: AlertPhase;
	effective_phase: AlertEffectivePhase;
	suppressed: boolean;
	/** Id of the unreachable parent device that masks this alert. */
	suppressed_by: TargetId | null;
	silenced: boolean;
	/** True while a baseline rule is still learning (silent for its first days). */
	learning: boolean;
	/**
	 * True while someone has acknowledged this alert: reminders and escalations
	 * pause until `acked_until`, the resolution is still notified. The phase
	 * does not move — an acked alert is still a problem, just a known one.
	 */
	acked: boolean;
	acked_until: string | null;
	/** Account that acknowledged, or `token:name` for an API token. */
	acked_by: string | null;
	ack_note: string | null;
	/**
	 * True once someone dismissed this occurrence: it left every list and
	 * count and nothing is sent for it. If the condition clears and comes
	 * back, that is a new alert. Only `GET /alerts?dismissed=true` returns these.
	 */
	dismissed: boolean;
	/** UTC, no suffix (`parseServerDate`). */
	dismissed_at: string | null;
	/** Account that dismissed, or `token:name` for an API token. */
	dismissed_by: string | null;
	value: number | null;
	score: number | null;
	condition_since: string | null;
	firing_since: string | null;
	last_eval_at: string | null;
	last_notified_at: string | null;
	/** Tickets opened for this alert by the ticket channels (Jira, GLPI, Zammad, GitHub, GitLab). */
	tickets?: AlertTicket[];
	/** The change marker of this monitor that preceded the alert by less than 30 minutes. */
	after_change?: AfterChange | null;
	notify_count: number;
}

/**
 * Body of `POST /api/alerts/{fingerprint}/ack`. One of `until` (RFC 3339) or
 * `duration_secs`; neither means four hours. `until: null` lifts the
 * acknowledgement, like `DELETE`.
 */
export interface AckPayload {
	until?: string | null;
	duration_secs?: number;
	note?: string | null;
}

/** One recorded phase transition, mirroring the server's `HistoryEntryView`. */
export interface AlertHistoryEntry {
	id: number;
	fingerprint: string;
	rule_uid: string;
	target_id: TargetId | null;
	from_phase: AlertPhase;
	to_phase: AlertPhase;
	severity: AlertSeverity;
	value: number | null;
	/** False when nothing was sent; `reason` then says why (learning, suppression…). */
	notified: boolean;
	reason: string;
	/** RFC 3339 timestamp in UTC. */
	at: string;
	/** Cleared from the active view by the user; the row itself is kept. */
	dismissed: boolean;
}

/** An alert rule, mirroring the server's `RuleView`. */
export interface AlertRule {
	id: number;
	uid: string;
	name: string;
	description: string;
	kind: RuleKind;
	query: string;
	operator: RuleOperator;
	threshold: number;
	/** Hysteresis: the alert clears only past this value. `null` = none. */
	clear_threshold: number | null;
	for_secs: number;
	severity: AlertSeverity;
	selector: TargetSelector;
	channels: number[];
	params: AnomalyParams;
	unit: string;
	repeat_secs: number | null;
	escalate_after_secs: number | null;
	enabled: boolean;
	/** Shipped rule: it can be toggled but not deleted. */
	builtin: boolean;
	/** Per-device overrides of this rule. */
	overrides: RuleOverride[];
	/** Last time the rule went to firing, from the alert history (UTC); `null` = never in the retained history. */
	last_fired_at: string | null;
}

/**
 * Body of `POST /api/alerts/rules` and `PUT /api/alerts/rules/{id}`.
 *
 * Enumerations travel as free strings (`severity`, `operator`, `kind`); the
 * server validates them and rejects a misspelling explicitly.
 */
export interface AlertRulePayload {
	uid?: string | null;
	name: string;
	description?: string | null;
	kind?: RuleKind | string | null;
	query: string;
	operator?: RuleOperator | string | null;
	threshold?: number | null;
	clear_threshold?: number | null;
	for_secs?: number | null;
	severity?: AlertSeverity | string | null;
	selector?: TargetSelector | null;
	channels?: number[] | null;
	params?: Partial<AnomalyParams> | null;
	unit?: string | null;
	repeat_secs?: number | null;
	escalate_after_secs?: number | null;
	enabled?: boolean | null;
}

/** Where a capacity forecast stands. */
export type ForecastStatus = 'growing' | 'stable' | 'full' | 'insufficient_data';

/**
 * Fill-up forecast of one capacity entity of a device (mountpoint, datastore,
 * pool, volume, licence…), from `GET /api/targets/{id}/forecasts`. Mirrors
 * `ForecastView` in `crates/server/src/api/forecasts.rs`.
 */
export interface CapacityForecast {
	/** Capacity family: `filesystem`, `pbs_datastore`, `truenas_pool`… */
	family: string;
	family_label: string;
	metric: string;
	/** Readable entity (`/srv`, `tank`…); empty for the whole device. */
	entity: string;
	labels: Record<string, string>;
	used_percent: number | null;
	/** Raw used amount in `unit`, when the integration publishes it. */
	used: number | null;
	size: number | null;
	/** `bytes`, `seats`… empty without raw amounts. */
	unit: string;
	/** Percentage points per day. */
	growth_per_day: number | null;
	/** `null` without growth or without enough history; 0 when full. */
	days_to_full: number | null;
	/** Estimated fill date, UTC without suffix (use `parseServerDate`). */
	full_at: string | null;
	/** 0…1. */
	confidence: number;
	status: ForecastStatus;
	/** What the forecast rule would raise today; `null` when it would not fire. */
	severity: AlertSeverity | null;
	window_days: number;
}

/** "First Sunday", "last Friday": `nth` is 1…5, or -1 for the last one. */
export interface NthWeekday {
	nth: number;
	/** 0 = Monday … 6 = Sunday. */
	weekday: number;
}

/** Maintenance-window schedule: one-off, weekly or monthly. */
export type SilenceSchedule =
	| { kind: 'once'; starts_at: string; ends_at: string }
	/** "Seen, I am on it": no end; the server lifts it when the monitor answers again. */
	| { kind: 'recovery'; starts_at: string }
	| {
			kind: 'weekly';
			/** Days it covers, 0 = Monday … 6 = Sunday. */
			days: number[];
			/** Minutes since local midnight, 0…1439. */
			start_minute: number;
			end_minute: number;
			/** Offset of the local timezone from UTC, in minutes. */
			utc_offset_minutes: number;
			/** IANA time zone ("Europe/Paris"). Wins over the fixed offset: only a
			 *  named zone knows where the daylight-saving changes fall. */
			timezone?: string | null;
	  }
	| {
			kind: 'monthly';
			/** Days of the month, 1…31. A month without that day is skipped. */
			days: number[];
			/** "First Sunday", "last Friday"… Added to `days` (OR). */
			nth_weekdays: NthWeekday[];
			/** Minutes since local midnight, 0…1439. */
			start_minute: number;
			/** How long the window lasts, in real minutes. */
			duration_minutes: number;
			utc_offset_minutes: number;
			timezone?: string | null;
	  };

/** A maintenance window, mirroring the server's `SilenceView`. */
export interface Silence {
	id: number;
	name: string;
	comment: string;
	target_id: TargetId | null;
	matchers: Record<string, string>;
	schedule: SilenceSchedule;
	enabled: boolean;
	/** True if the window covers the present instant (computed by the server). */
	active_now: boolean;
	/** End of the occurrence in progress, when there is one. */
	active_until: string | null;
	/** Start of the next occurrence; the server unrolls the calendar. */
	next_start_at: string | null;
}

/** One dated occurrence of a maintenance window (`GET /api/alerts/silences/occurrences`). */
export interface SilenceOccurrence {
	silence_id: number;
	name: string;
	starts_at: string;
	ends_at: string;
	state: 'active' | 'upcoming' | 'past';
	/** True for a weekly or monthly rule. */
	recurring: boolean;
	enabled: boolean;
}

/** Body of `POST /api/alerts/silences`. */
export interface SilencePayload {
	name: string;
	comment?: string | null;
	target_id?: TargetId | null;
	matchers?: Record<string, string>;
	schedule: SilenceSchedule;
	enabled?: boolean | null;
}
