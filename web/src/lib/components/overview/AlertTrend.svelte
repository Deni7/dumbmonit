<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Incidents of the last seven days, at the top of the Overview: how many
	 * there were and how long they stayed open, against the week before, one
	 * bar per day, and the monitor behind the most of them. An incident starts
	 * when an alert fires and ends when it resolves (the sentence under the
	 * title says so). Read from the alert history the fleet store already
	 * holds, so it costs no query of its own.
	 */
	import { formatDuration } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Skeleton } from '#lib/ui/index.js';
	import { fleetStore } from '#lib/components/enterprise/fleet.svelte.js';
	import { incidentWeek } from './attention.js';

	const DAYS = 7;

	const week = $derived(incidentWeek(fleetStore.history, Date.now(), DAYS));
	const peak = $derived(Math.max(1, ...week.days.map((day) => day.count)));
	const noisiest = $derived(
		week.noisiest && week.noisiest.count >= 2
			? { ...week.noisiest, name: alertsStore.targets.find((t) => t.id === week.noisiest?.target)?.name ?? null }
			: null
	);
	const weekday = (day: number) => new Date(day * 1000).toLocaleDateString(undefined, { weekday: 'short' });
</script>

<section class="min-w-0 rounded-xl border border-line bg-surface px-4 pt-3.5 pb-4" aria-labelledby="incidents-title">
	<h2 id="incidents-title" class="m-0 text-base font-semibold text-ink">{m.overview_incidents_title({ days: DAYS })}</h2>
	<p class="m-0 mt-0.5 text-sm text-ink-2">{m.overview_incidents_explain()}</p>

	{#if fleetStore.loading && fleetStore.history.length === 0}
		<Skeleton class="mt-3 h-24 w-full" />
	{:else if week.count === 0}
		<p class="m-0 mt-3 text-sm font-medium text-signal-ink">{m.overview_incidents_none({ days: DAYS })}</p>
	{:else}
		<div class="mt-3 grid gap-x-8 gap-y-4 md:grid-cols-[minmax(0,auto)_minmax(0,1fr)] md:items-end">
			<dl class="m-0 grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-3 md:grid-cols-2">
				<div class="min-w-0">
					<dt class="text-sm text-ink-2">{m.overview_incidents_count_label()}</dt>
					<dd class="m-0 mt-1 font-mono text-2xl leading-none font-medium tracking-[-0.03em] text-ink tabular-nums">{week.count}</dd>
					<dd class="m-0 mt-1 text-xs text-ink-2">{m.overview_incidents_before({ value: String(week.previousCount) })}</dd>
				</div>
				<div class="min-w-0">
					<dt class="text-sm text-ink-2">{m.overview_incidents_open_label()}</dt>
					<dd class="m-0 mt-1 font-mono text-2xl leading-none font-medium tracking-[-0.03em] text-ink tabular-nums">{formatDuration(week.seconds)}</dd>
					<dd class="m-0 mt-1 text-xs text-ink-2">{m.overview_incidents_before({ value: formatDuration(week.previousSeconds) })}</dd>
				</div>
				{#if noisiest?.name}
					<div class="col-span-2 min-w-0 sm:col-span-1 md:col-span-2">
						<dt class="text-sm text-ink-2">{m.overview_incidents_noisiest()}</dt>
						<dd class="m-0 mt-1 flex min-w-0 items-baseline gap-2">
							<a href={withBase(`/targets/${noisiest.target}`)} class="inline-flex min-h-10 min-w-0 items-center truncate text-sm font-semibold text-ink hover:underline">{noisiest.name}</a>
							<span class="font-mono text-xs text-ink-2 tabular-nums" title={m.overview_incidents_count_label()}>{noisiest.count}</span>
						</dd>
					</div>
				{/if}
			</dl>

			<ul class="m-0 grid h-32 min-w-0 list-none grid-cols-7 gap-1.5 p-0 sm:gap-3" aria-label={m.overview_incidents_bars_label()}>
				{#each week.days as day, i (day.day)}
					{@const title = m.overview_incidents_day_title({ day: weekday(day.day), count: day.count, duration: formatDuration(day.seconds) })}
					<li class="flex min-w-0 flex-col items-center justify-end gap-1" {title} aria-label={title}>
						<span class="font-mono text-xs text-ink-2 tabular-nums" aria-hidden="true">{day.count}</span>
						<span class="flex min-h-0 w-full flex-1 items-end" aria-hidden="true">
							<span
								class={`block w-full rounded-sm ${day.count === 0 ? 'bg-line-strong' : i === week.days.length - 1 ? 'bg-warning' : 'bg-warning/55'}`}
								style={`height: ${day.count === 0 ? 2 : Math.max(6, Math.round((day.count / peak) * 100))}%`}
							></span>
						</span>
						<span class={`text-xs ${i === week.days.length - 1 ? 'font-semibold text-ink' : 'text-ink-2'}`} aria-hidden="true">{weekday(day.day)}</span>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</section>
