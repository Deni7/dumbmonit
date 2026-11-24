/**
 * Testing a rule on history and the alert noise report. Mirrors
 * `crates/server/src/api/alert_backtest.rs` and `alert_noise.rs`.
 * The backtest is an admin route (it takes the rule being edited); the noise
 * report is for admins and operators.
 */
import { request } from './client';
import type { AlertRulePayload } from './types';

/** Windows the backtest accepts, in days. */
export const BACKTEST_DAYS = [1, 7, 30] as const;
export type BacktestDays = (typeof BACKTEST_DAYS)[number];

/** Windows of the noise report, in days. */
export const NOISE_DAYS = [7, 30, 90] as const;
export type NoiseDays = (typeof NOISE_DAYS)[number];

/** Rule kinds that cannot be replayed, as the server names them. */
export type NotReplayableCode = 'anomaly' | 'log_pattern' | 'forecast' | 'drift' | 'slo_burn';

export interface BacktestPeriod {
	/** Unix seconds. */
	start: number;
	/** Unix seconds; `null` while still firing at the end of the window. */
	end: number | null;
	/** Worst value of the period (highest for `>`, lowest for `<`). */
	peak: number;
}

export interface BacktestSeries {
	series_key: string;
	target_id: number | null;
	target_name: string;
	/** Threshold applied to this series, per-device override included. */
	threshold: number;
	clear_threshold: number | null;
	periods: BacktestPeriod[];
	notifications: number;
	/** Breaches that cleared before the hold duration. */
	absorbed: number;
	firing_secs: number;
	/** The window summed up in at most 96 values. */
	spark: number[];
}

export interface RuleBacktest {
	replayable: boolean;
	not_replayable: { code: NotReplayableCode | string; reason: string } | null;
	days: number;
	/** Unix seconds. */
	start: number;
	end: number;
	step_secs: number;
	interval_secs: number;
	/** The step was widened to bound the cost. */
	coarsened: boolean;
	series_total: number;
	/** Not every series was replayed. */
	truncated: boolean;
	fired: number;
	notifications: number;
	absorbed: number;
	/** Replayed series that would never have fired. */
	quiet_series: number;
	/** Series that would have fired, noisiest first. */
	series: BacktestSeries[];
}

export interface RuleBacktestRequest {
	rule: AlertRulePayload;
	/** The saved rule being edited; omitted for a rule not saved yet. */
	rule_id?: number | null;
	days?: BacktestDays;
}

export interface AlertNoiseStats {
	fired: number;
	notifications: number;
	acknowledged: number;
	resolved: number;
	/** Mean time to acknowledge, seconds; `null` without any acknowledgement. */
	mtta_secs: number | null;
	/** Mean time to resolve, seconds; `null` without any resolution. */
	mttr_secs: number | null;
}

export interface AlertNoiseRule extends AlertNoiseStats {
	rule_uid: string;
	/** `null` once the rule is deleted. */
	rule_id: number | null;
	rule_name: string;
	/** The rule can be tested on history. */
	replayable: boolean;
}

export interface AlertNoiseTeam extends AlertNoiseStats {
	team_id: number;
	team_name: string;
}

export interface AlertNoiseFlapping {
	fingerprint: string;
	rule_uid: string;
	rule_id: number | null;
	rule_name: string;
	target_id: number | null;
	target_name: string | null;
	/** Transitions to firing or resolved in the window. */
	changes: number;
	firings: number;
}

export interface AlertNoise {
	days: number;
	/** Start of the window, UTC without suffix (use `parseServerDate`). */
	since: string;
	/** More episodes than the report reads: it covers the most recent. */
	truncated: boolean;
	overall: AlertNoiseStats;
	rules: AlertNoiseRule[];
	flapping: AlertNoiseFlapping[];
	teams: AlertNoiseTeam[];
}

export function backtestAlertRule(payload: RuleBacktestRequest, signal?: AbortSignal): Promise<RuleBacktest> {
	return request<RuleBacktest>('/alerts/rules/backtest', { method: 'POST', body: payload, signal });
}

export function getAlertNoise(days: NoiseDays, signal?: AbortSignal): Promise<AlertNoise> {
	return request<AlertNoise>('/alerts/noise', { query: { days }, signal });
}
