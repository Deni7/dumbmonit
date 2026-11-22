<script lang="ts">
	/**
	 * "Test on history", inside the rule editor: replays the rule as the form
	 * now stands — saved or not — over the last day, 7 or 30 days of stored
	 * measurements, with the engine's own threshold, clear threshold and hold
	 * logic, and says what it would have done: when it would have fired, for
	 * how long, how high, and how many notifications that means.
	 *
	 * The form validates itself (`getPayload` returns null and shows why).
	 * The counts are the rule's alone: maintenance windows, dependencies and
	 * channel settings are left out, and the panel says so.
	 */
	import { onDestroy, untrack } from 'svelte';
	import type { AlertRule, AlertRulePayload, BacktestDays, RuleBacktest } from '#lib/api/index.js';
	import { BACKTEST_DAYS, backtestAlertRule } from '#lib/api/index.js';
	import { Button } from '#lib/ui/index.js';
	import Segmented from '#lib/components/devices/Segmented.svelte';
	import { formatDateTime, formatDuration } from '#lib/format.js';
	import { formatAlertValue } from '../helpers';
	import { m } from '#lib/paraglide/messages.js';
	import BacktestTimeline from './BacktestTimeline.svelte';

	interface Props {
		rule: AlertRule;
		getPayload: () => AlertRulePayload | null;
		/** Run once as soon as the editor opens. */
		autoRun?: boolean;
	}

	let { rule, getPayload, autoRun = false }: Props = $props();

	/** Series shown before "Show all". */
	const SHOWN = 8;
	/** Periods written out per series. */
	const PERIODS_SHOWN = 3;

	let days = $state<BacktestDays>(7);
	let running = $state(false);
	let result = $state<RuleBacktest | null>(null);
	let failure = $state<string | null>(null);
	let showAll = $state(false);
	let controller: AbortController | null = null;
	onDestroy(() => controller?.abort());

	const dayOptions = $derived(BACKTEST_DAYS.map((n) => ({ id: String(n) as `${BacktestDays}`, label: m.w5alerting_backtest_window({ count: n }) })));

	const panelId = $derived(`rule-${rule.id}-backtest`);
	const shownSeries = $derived(result ? (showAll ? result.series : result.series.slice(0, SHOWN)) : []);
	const value = (v: number) => formatAlertValue(v, rule.unit);
	const at = (seconds: number) => formatDateTime(new Date(seconds * 1000));

	const notReplayable = $derived.by(() => {
		const why = result?.not_replayable;
		if (!why) return null;
		switch (why.code) {
			case 'anomaly':
				return m.w5alerting_backtest_not_anomaly();
			case 'log_pattern':
				return m.w5alerting_backtest_not_log_pattern();
			case 'forecast':
				return m.w5alerting_backtest_not_forecast();
			case 'drift':
				return m.w5alerting_backtest_not_drift();
			case 'slo_burn':
				return m.w5alerting_backtest_not_slo_burn();
			default:
				// A kind this interface does not know yet: the server's own words.
				return why.reason;
		}
	});

	async function run() {
		failure = null;
		const payload = getPayload();
		if (!payload) return;
		controller?.abort();
		controller = new AbortController();
		const signal = controller.signal;
		running = true;
		try {
			result = await backtestAlertRule({ rule: payload, rule_id: rule.id, days }, signal);
			showAll = false;
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			failure = cause instanceof Error ? cause.message : m.w5alerting_backtest_error();
		} finally {
			if (!signal.aborted) running = false;
		}
	}

	// Opened from the noise report's "Test on history": once, with the saved values.
	let autoRan = false;
	$effect(() => {
		if (!autoRun || autoRan) return;
		autoRan = true;
		untrack(() => void run());
	});
</script>

<section class="grid gap-3 border-t border-line pt-4" aria-labelledby={`${panelId}-title`}>
	<div class="flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
		<div class="min-w-[min(100%,16rem)] flex-1">
			<h3 id={`${panelId}-title`} class="text-sm font-semibold text-ink">{m.w5alerting_backtest_title()}</h3>
			<p class="text-[0.8125rem] text-ink-2">{m.w5alerting_backtest_help()}</p>
		</div>
		<div class="flex flex-wrap items-center gap-2">
			<Segmented
				options={dayOptions}
				value={String(days) as `${BacktestDays}`}
				onchange={(v) => (days = Number(v) as BacktestDays)}
				label={m.w5alerting_backtest_window_label()}
				size="sm"
			/>
			<Button type="button" size="sm" loading={running} onclick={run}>{m.w5alerting_backtest_run()}</Button>
		</div>
	</div>

	<div aria-live="polite" class="grid gap-3">
		{#if failure}
			<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{failure}</p>
		{:else if result && !result.replayable}
			<p class="text-[0.8125rem] text-ink-2">{notReplayable}</p>
		{:else if result}
			<div class="grid gap-1 text-sm text-ink">
				{#if result.fired === 0}
					<p class="font-semibold">{m.w5alerting_backtest_never_fired({ count: result.days })}</p>
				{:else}
					<p class="font-semibold">{m.w5alerting_backtest_fired({ count: result.fired })}</p>
					<p>{m.w5alerting_backtest_notifications({ count: result.notifications })}</p>
				{/if}
				{#if result.absorbed > 0}
					<p class="text-ink-2">{m.w5alerting_backtest_absorbed({ count: result.absorbed })}</p>
				{/if}
				{#if result.fired > 0 && result.quiet_series > 0}
					<p class="text-ink-2">{m.w5alerting_backtest_quiet({ count: result.quiet_series })}</p>
				{/if}
			</div>
			{#if result.coarsened || result.truncated}
				<ul class="grid gap-0.5 text-[0.8125rem] text-ink-2">
					{#if result.coarsened}
						<li>{m.w5alerting_backtest_coarsened({ step: formatDuration(result.step_secs), interval: formatDuration(result.interval_secs) })}</li>
					{/if}
					{#if result.truncated}
						<li>{m.w5alerting_backtest_truncated({ count: result.series_total })}</li>
					{/if}
				</ul>
			{/if}

			{#if shownSeries.length > 0}
				<ul class="grid gap-3">
					{#each shownSeries as series (series.series_key)}
						<li class="grid gap-2 rounded-[var(--radius-plate)] border border-line bg-canvas px-3 py-2.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start sm:gap-4">
							<div class="min-w-0">
								<p class="truncate text-sm font-semibold text-ink">{series.target_name}</p>
								<p class="truncate font-mono text-[0.75rem] text-ink-3" title={series.series_key}>{series.series_key}</p>
								<p class="mt-1 text-[0.8125rem] text-ink-2">
									{m.w5alerting_backtest_series_periods({ count: series.periods.length, duration: formatDuration(series.firing_secs) })}
								</p>
								<p class="text-[0.8125rem] text-ink-2">{m.w5alerting_backtest_series_notifications({ count: series.notifications })}</p>
								<ul class="mt-1 grid gap-0.5 text-[0.75rem] text-ink-2 tnum">
									{#each series.periods.slice(0, PERIODS_SHOWN) as period (period.start)}
										<li>
											{period.end === null
												? m.w5alerting_backtest_period_open({ start: at(period.start), peak: value(period.peak) })
												: m.w5alerting_backtest_period({ start: at(period.start), end: at(period.end), peak: value(period.peak) })}
										</li>
									{/each}
									{#if series.periods.length > PERIODS_SHOWN}
										<li>{m.w5alerting_backtest_period_more({ count: series.periods.length - PERIODS_SHOWN })}</li>
									{/if}
								</ul>
							</div>
							<BacktestTimeline
								values={series.spark}
								periods={series.periods}
								start={result.start}
								end={result.end}
								threshold={series.threshold}
								label={m.w5alerting_backtest_spark_label({ name: series.target_name })}
								format={value}
							/>
						</li>
					{/each}
				</ul>
				{#if result.series.length > SHOWN}
					<div>
						<Button type="button" variant="ghost" size="sm" onclick={() => (showAll = !showAll)} aria-expanded={showAll}>
							{showAll ? m.w5alerting_backtest_show_less() : m.w5alerting_backtest_show_all({ count: result.series.length })}
						</Button>
					</div>
				{/if}
			{/if}
			<p class="text-[0.75rem] text-ink-3">{m.w5alerting_backtest_scope_note()}</p>
		{/if}
	</div>
</section>
