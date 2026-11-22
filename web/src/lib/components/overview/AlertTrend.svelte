<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Incidents of the last seven days, in the right-hand column of the Overview: how many
	 * there were and how long they stayed open, against the week before, one
	 * bar per day, and the monitor behind the most of them. An incident starts
	 * when an alert fires and ends when it resolves (the sentence under the
	 * title says so). The period is 7, 14 or 30 days, remembered in this browser.
	 * Read from the alert history the fleet store already holds (the last 60 days,
	 * so the period before can be compared), so it costs no query of its own.
	 */
	import { formatDuration } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Segmented, Skeleton } from '#lib/ui/index.js';
	import { fleetStore } from '#lib/components/enterprise/fleet.svelte.js';
	import { incidentWeek } from './attention.js';

	const PERIODS = [7, 14, 30] as const;
	const PERIOD_KEY = 'dumbmonit-overview-trend-days';
	function readDays(): number {
		try {
			const saved = Number(localStorage.getItem(PERIOD_KEY));
			return (PERIODS as readonly number[]).includes(saved) ? saved : 7;
		} catch {
			return 7;
		}
	}
	let days = $state(readDays());
	let choice = $state(String(days));
	function pick(value: string) {
		days = Number(value);
		try {
			localStorage.setItem(PERIOD_KEY, value);
		} catch {
			// Not remembered: back to seven days on the next visit.
		}
	}
	const periodOptions = $derived(PERIODS.map((n) => ({ value: String(n), label: m.w1wall_overview_trend_days({ days: n }) })));

	const week = $derived(incidentWeek(fleetStore.history, Date.now(), days));
	// A floor of four: a day with one incident stays a short bar instead of filling the chart.
	const peak = $derived(Math.max(4, ...week.days.map((day) => day.count)));

	/** "32 d 7 h" from two days up; shorter spans keep the usual one-figure form. */
	function span(seconds: number): string {
		if (seconds < 2 * 86400) return formatDuration(seconds);
		const days = Math.floor(seconds / 86400);
		const hours = Math.round((seconds - days * 86400) / 3600);
		return hours === 0 || hours === 24
			? m.misc_dur_d({ n: hours === 24 ? days + 1 : days })
			: `${m.misc_dur_d({ n: days })} ${m.misc_dur_h({ n: hours })}`;
	}
	const noisiest = $derived(
		week.noisiest && week.noisiest.count >= 2
			? { ...week.noisiest, name: alertsStore.targets.find((t) => t.id === week.noisiest?.target)?.name ?? null }
			: null
	);
	const weekday = (day: number) => new Date(day * 1000).toLocaleDateString(undefined, { weekday: 'short' });
	/** Under the bars: weekdays for a week, day numbers beyond, and only every fifth over a month. */
	function barLabel(day: number, i: number, count: number): string {
		if (days <= 7) return weekday(day);
		if (days > 14 && i !== count - 1 && i % 5 !== 0) return '';
		return String(new Date(day * 1000).getDate());
	}
</script>

<section class="flex min-w-0 flex-col rounded-xl border border-line bg-surface px-4 pt-3.5 pb-4" aria-labelledby="incidents-title">
	<div class="flex flex-wrap items-start justify-between gap-x-3 gap-y-2">
		<h2 id="incidents-title" class="m-0 text-base font-semibold text-ink">{m.overview_incidents_title({ days })}</h2>
		<Segmented legend={m.w1wall_overview_trend_period()} hideLegend name="overview-trend-days" options={periodOptions} bind:value={choice} onchange={pick} />
	</div>
	<p class="m-0 mt-0.5 text-sm text-ink-2">{m.overview_incidents_explain()}</p>

	{#if fleetStore.loading && fleetStore.history.length === 0}
		<Skeleton class="mt-3 h-28 w-full" />
	{:else if week.count === 0}
		<p class="m-0 mt-3 text-sm font-medium text-signal-ink">{m.overview_incidents_none({ days })}</p>
	{:else}
		<dl class="m-0 mt-3 grid grid-cols-2 gap-x-6 gap-y-2.5">
			<div class="min-w-0">
				<dt class="text-sm text-ink-2">{m.overview_incidents_count_label()}</dt>
				<dd class="m-0 mt-1 font-mono text-xl leading-none font-medium tracking-[-0.03em] text-ink tabular-nums">{week.count}</dd>
				<dd class="m-0 mt-1 text-xs text-ink-2">{m.overview_incidents_before({ value: String(week.previousCount) })}</dd>
			</div>
			<div class="min-w-0">
				<dt class="text-sm text-ink-2">{m.overview_incidents_open_label()}</dt>
				<dd class="m-0 mt-1 font-mono text-xl leading-none font-medium tracking-[-0.03em] text-ink tabular-nums">{span(week.seconds)}</dd>
				<dd class="m-0 mt-1 text-xs text-ink-2">{m.overview_incidents_before({ value: span(week.previousSeconds) })}</dd>
			</div>
			{#if noisiest?.name}
				<div class="col-span-2 min-w-0">
					<dt class="text-sm text-ink-2">{m.overview_incidents_noisiest()}</dt>
					<dd class="m-0 mt-1 flex min-w-0 items-baseline gap-2">
						<a href={withBase(`/targets/${noisiest.target}`)} class="inline-flex min-h-10 min-w-0 items-center truncate text-sm font-semibold text-ink hover:underline">{noisiest.name}</a>
						<span class="font-mono text-xs text-ink-2 tabular-nums" title={m.overview_incidents_count_label()}>{noisiest.count}</span>
					</dd>
				</div>
			{/if}
		</dl>

		<ul class="m-0 mt-3 grid h-28 min-h-28 max-h-64 min-w-0 flex-1 list-none gap-1.5 p-0 sm:gap-2" style={`grid-template-columns: repeat(${days}, minmax(0, 1fr)); ${days > 14 ? 'column-gap: 0.125rem;' : ''}`} aria-label={m.overview_incidents_bars_label()}>
			{#each week.days as day, i (day.day)}
				{@const title = m.overview_incidents_day_title({ day: weekday(day.day), count: day.count, duration: span(day.seconds) })}
				<li class="flex min-w-0 flex-col items-center justify-end gap-1" {title} aria-label={title}>
					<span class="font-mono text-xs text-ink-2 tabular-nums" aria-hidden="true">{days <= 14 ? day.count : ''}</span>
					<span class="flex min-h-0 w-full flex-1 items-end" aria-hidden="true">
						<span
							class={`block w-full rounded-sm ${day.count === 0 ? 'bg-line-strong' : i === week.days.length - 1 ? 'bg-warning' : 'bg-warning/55'}`}
							style={`height: ${day.count === 0 ? 2 : Math.max(8, Math.round((day.count / peak) * 100))}%`}
						></span>
					</span>
					<span class={`text-xs ${i === week.days.length - 1 ? 'font-semibold text-ink' : 'text-ink-2'}`} aria-hidden="true">{barLabel(day.day, i, week.days.length)}</span>
				</li>
			{/each}
		</ul>
	{/if}
</section>
