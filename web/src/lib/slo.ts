/**
 * Service level objectives: the words and tones the SLO tab draws from the
 * server's numbers. Pure: no network, no messages, so tests load it directly.
 * The arithmetic mirrors `crates/server/src/slo/mod.rs`.
 */
import type { SloBurnRates, SloStatus, SloVerdict } from './api/types';

export type SloTone = 'signal' | 'advisory' | 'warning' | 'ghost';

/** Burn-rate multipliers of the alerts (Google SRE, multi-window). */
export const FAST_BURN = 14.4;
export const SLOW_BURN = 6;

/** Below this share of the budget left, the bar turns to an advisory. */
export const LOW_BUDGET_PERCENT = 25;

/** Share of the time a target allows to be down: 0.001 for 99.9 %. */
export function allowedError(objective: number): number {
	return Math.max(0, (100 - objective) / 100);
}

/** Downtime the target allows over the window, in minutes (43.2 for 99.9 % over 30 days). */
export function allowedMinutes(objective: number, windowDays: number): number {
	return windowDays * 1440 * allowedError(objective);
}

/** The target as typed ("99,9" or "99.9"); `null` unless strictly between 0 and 100. */
export function parseObjective(raw: string): number | null {
	const trimmed = raw.trim().replace(',', '.');
	if (!/^\d+(\.\d+)?$/.test(trimmed)) return null;
	const value = Number(trimmed);
	return value > 0 && value < 100 ? value : null;
}

/** Width of the budget bar, in percent of its track: an overspent budget is an empty bar. */
export function budgetFill(remainingPercent: number | null | undefined): number {
	if (remainingPercent === null || remainingPercent === undefined || !Number.isFinite(remainingPercent)) return 0;
	return Math.min(100, Math.max(0, remainingPercent));
}

/** Tone of the budget: spent is a warning, running low an advisory, unmeasured a ghost. */
export function budgetTone(status: SloStatus): SloTone {
	const left = status.budget?.remaining_percent;
	if (left === undefined || left === null) return 'ghost';
	if (left <= 0) return 'warning';
	if (left < LOW_BUDGET_PERCENT) return 'advisory';
	return 'signal';
}

/** Tone of a burn verdict: the same ladder as the alerts it raises. */
export function verdictTone(verdict: SloVerdict): SloTone {
	return verdict === 'fast' ? 'warning' : verdict === 'slow' ? 'advisory' : 'signal';
}

/** The rate a row shows: the window that decided the verdict, else the last hour. */
export function shownRate(burn: SloBurnRates, verdict: SloVerdict): number | null {
	if (verdict === 'slow') return burn.h6;
	return burn.h1 ?? burn.h6;
}

/** "15×", "1.2×", "0×": two significant digits are enough. */
export function formatRate(rate: number | null | undefined): string | null {
	if (rate === null || rate === undefined || !Number.isFinite(rate)) return null;
	if (rate >= 10) return `${Math.round(rate)}×`;
	if (rate === 0) return '0×';
	return `${Math.round(rate * 10) / 10}×`;
}

/** An availability with as many decimals as the target needs (99.95 → 3, 99 → 2). */
export function formatAvailability(value: number, objective: number): string {
	const decimals = Math.min(4, Math.max(2, (String(objective).split('.')[1] ?? '').length + 1));
	return `${value.toFixed(decimals)} %`;
}
