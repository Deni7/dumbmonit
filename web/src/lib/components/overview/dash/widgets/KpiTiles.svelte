<script lang="ts">
	/**
	 * The three figures at the head of every dashboard, each with what makes it
	 * readable at a glance: monitors up with one bar per monitor grouped by
	 * site; availability over 30 days with its objective, the trend and the
	 * error budget left; open alerts with their severities, the alerts opened
	 * per day over two weeks and the week against the one before.
	 *
	 * Monitors and alerts come from the app-wide store (the same counts as the
	 * classic overview and the nav badge); availability and the 14 days from
	 * the dashboards' store. Not a `WidgetCard`: a row of three tiles that
	 * stacks on a phone. `band`: drawn inside the dark band of the hybrid layout.
	 */
	import { countAlertsByTarget, countsAsProblem } from '#lib/alerts-attention.js';
	import { formatRelative } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Plate, Skeleton } from '#lib/ui/index.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { availabilityText, barWidth, budgetTone, monitorBars, pointDelta, signed } from '../parts/format.js';
	import { STATUS_FILL, type DashStatus } from '../parts/plates.js';
	import { groupBySite } from '../parts/sites.js';
	import { dayText, statusWord } from '../parts/words.js';

	interface Props {
		band?: boolean;
		class?: string;
	}

	let { band = false, class: className = '' }: Props = $props();

	const data = $derived(widgetsStore.data);
	const reading = $derived(alertsStore.loading && alertsStore.targets.length === 0);

	// --- Monitors up ----------------------------------------------------------
	const alertCounts = $derived(countAlertsByTarget(alertsStore.alerts));
	const monitors = $derived(
		alertsStore.targets.map((target) => ({
			id: target.id,
			name: target.name,
			status: consoleStatus(target, alertsStore.probes.get(target.id), alertCounts.get(target.id)) as DashStatus
		}))
	);
	const active = $derived(monitors.filter((r) => r.status !== 'paused').length);
	const up = $derived(monitors.filter((r) => r.status === 'up' || r.status === 'degraded').length);
	const down = $derived(monitors.filter((r) => r.status === 'down').length);
	const paused = $derived(monitors.length - active);
	const groups = $derived(groupBySite(monitors, data?.sites ?? [], m.w9dash_common_this_site()));
	const strip = $derived(
		monitorBars(groups.map((g) => ({ key: g.key, name: g.name, statuses: g.monitors.map((r) => r.status) })))
	);
	/** Names for the bars' tooltips, in the strip's order. */
	const stripNames = $derived(groups.flatMap((g) => g.monitors.map((r) => r.name)));

	// --- Availability ---------------------------------------------------------
	const availability = $derived(data?.availability ?? null);
	const availDelta = $derived(availability ? pointDelta(availability.avg_30d, availability.previous_avg_30d) : null);
	const availValues = $derived(availability?.days.map((d) => d.pct) ?? []);
	const availLow = $derived.by(() => {
		const target = availability?.slo_target ?? 99.9;
		const floor = target - Math.max(0.1, 100 - target) * 4;
		const seen = availValues.filter((v): v is number => v !== null);
		return Math.max(0, Math.min(floor, ...seen));
	});
	const budget = $derived(availability?.budget_left_pct ?? null);
	const budgetState = $derived(budgetTone(budget));

	// --- Open alerts ----------------------------------------------------------
	const firing = $derived(alertsStore.alerts.filter(countsAsProblem));
	const critical = $derived(firing.filter((a) => a.severity === 'critical').length);
	const warning = $derived(firing.length - critical);
	const opened = $derived(data?.alerts_opened ?? null);
	const openedPeak = $derived(Math.max(4, ...(opened?.days.map((d) => d.count) ?? [0])));

	const tile = $derived(
		`grid min-w-0 grid-cols-[minmax(0,1fr)_auto] content-start items-baseline rounded-[var(--radius-card)] border bg-surface px-4 pt-3.5 pb-3 ${band ? 'border-line-strong' : 'border-line'}`
	);
	const figure = 'font-mono text-[2.25rem] leading-none font-medium tracking-[-0.04em] text-ink tabular-nums sm:text-[2.625rem]';
</script>

<div class={`grid grid-cols-1 gap-3.5 md:grid-cols-3 ${className}`}>
	{#if reading}
		{#each [0, 1, 2] as i (i)}
			<div class={tile} aria-hidden="true">
				<Skeleton class="col-span-2 h-4 w-32" />
				<Skeleton class="col-span-2 mt-3 h-10 w-24" />
				<Skeleton class="col-span-2 mt-3 h-6 w-full" />
			</div>
		{/each}
	{:else}
		<!-- Monitors up -->
		<section class={tile} aria-label={m.console_kpi_up()}>
			<h2 class="m-0 truncate text-sm font-semibold text-ink-2">{m.console_kpi_up()}</h2>
			{#if groups.length > 1}
				<span class="justify-self-end text-xs font-medium text-ink-3">{m.w9dash_kpi_sites({ count: groups.length })}</span>
			{:else}
				<span></span>
			{/if}
			<p class="col-span-2 m-0 mt-1.5 flex items-baseline gap-1.5">
				<span class={figure}>{up}</span>
				<span class="text-base text-ink-3">{m.console_kpi_up_unit({ total: active })}</span>
			</p>
			<div class="col-span-2 mt-2 flex min-h-6 flex-wrap items-center gap-1.5 text-[0.8125rem] text-ink-2">
				{#if down > 0}
					<Plate tone="warning" label={m.w9dash_kpi_down({ count: down })} />
				{:else if active > 0}
					<Plate tone="signal" label={m.w9dash_kpi_all_up()} />
				{/if}
				{#if paused > 0}<span>{m.w9dash_kpi_paused({ count: paused })}</span>{/if}
			</div>
			{#if strip.bars.length > 0}
				<div class="col-span-2 mt-2.5 min-w-0">
					<svg viewBox={`0 0 ${strip.width} 26`} preserveAspectRatio="none" class="block h-[26px] w-full" role="img" aria-label={m.w9dash_kpi_bars_label()}>
						{#each strip.bars as bar, i (i)}
							<rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx="1" fill={STATUS_FILL[bar.status as DashStatus]}>
								<title>{stripNames[i]}: {statusWord(bar.status as DashStatus)}</title>
							</rect>
						{/each}
					</svg>
					{#if strip.labels.length > 1}
						<div class="mt-1 flex gap-2 text-[0.71875rem] text-ink-3">
							{#each strip.labels as label (label.key)}
								<span class="min-w-0 truncate" style={`flex:${label.share} 1 0`}>{label.name}</span>
							{/each}
						</div>
					{/if}
				</div>
			{/if}
		</section>

		<!-- Availability, 30 days -->
		<section class={tile} aria-label={m.console_kpi_availability()}>
			<h2 class="m-0 truncate text-sm font-semibold text-ink-2">{m.console_kpi_availability()}</h2>
			{#if availDelta !== null}
				<span class={`max-w-[62%] justify-self-end text-right text-xs leading-tight font-semibold ${availDelta > 0 ? 'text-signal-ink' : availDelta < 0 ? 'text-advisory-ink' : 'text-ink-2'}`}>
					{m.w9dash_kpi_avail_delta({ delta: signed(availDelta) })}
				</span>
			{:else}
				<span></span>
			{/if}
			<p class="col-span-2 m-0 mt-1.5 flex items-baseline gap-1.5">
				<span class={figure}>{availabilityText(availability?.avg_30d)}</span>
				{#if availability?.avg_30d != null}<span class="text-base text-ink-3">%</span>{/if}
			</p>
			<div class="col-span-2 mt-2 flex min-h-6 flex-wrap items-center gap-1.5 text-[0.8125rem] text-ink-2">
				{#if availability}
					<span>{m.w9dash_kpi_avail_note({ target: availability.slo_target })}</span>
				{:else if widgetsStore.error}
					<span class="text-advisory-ink" role="alert">{m.w9dash_common_error()}</span>
				{:else if widgetsStore.loading}
					<Skeleton class="h-4 w-40" />
				{:else}
					<span>{m.console_kpi_none()}</span>
				{/if}
			</div>
			{#if availability}
				<div class="col-span-2 mt-2.5 min-w-0">
					<Spark values={availValues} height={34} min={availLow} max={100} ref={availability.slo_target} color="var(--color-info)" />
				</div>
				{#if budget !== null}
					<div class="col-span-2 mt-1.5 flex justify-between gap-2 text-xs text-ink-3">
						<span>{m.w9dash_kpi_budget_left()}</span>
						<span class={`font-mono font-semibold tabular-nums ${budgetState === 'ok' ? 'text-ink' : 'text-advisory-ink'}`}>{Math.round(budget)}%</span>
					</div>
					<div class="col-span-2 mt-1 h-1.5 overflow-hidden rounded-full bg-ghost">
						<i class={`block h-full rounded-full ${budgetState === 'ok' ? 'bg-signal' : 'bg-advisory'}`} style={`width:${barWidth(budget)}%`}></i>
					</div>
				{/if}
			{/if}
		</section>

		<!-- Open alerts -->
		<section class={tile} aria-label={m.console_kpi_alerts()}>
			<h2 class="m-0 truncate text-sm font-semibold text-ink-2">{m.console_kpi_alerts()}</h2>
			{#if opened}
				<span class="max-w-[62%] justify-self-end text-right text-xs leading-tight font-semibold text-ink-2">
					{m.w9dash_kpi_opened_week({ count: opened.this_week, previous: opened.last_week })}
				</span>
			{:else}
				<span></span>
			{/if}
			<p class="col-span-2 m-0 mt-1.5 flex items-baseline gap-1.5">
				<span class={figure}>{firing.length}</span>
			</p>
			<div class="col-span-2 mt-2 flex min-h-6 flex-wrap items-center gap-1.5 text-[0.8125rem] text-ink-2">
				{#if firing.length > 0}
					{#if critical > 0}<Plate tone="warning" label={m.w9dash_kpi_critical({ count: critical })} />{/if}
					{#if warning > 0}<Plate tone="advisory" label={m.w9dash_kpi_warning({ count: warning })} />{/if}
				{:else}
					<Plate tone="signal" label={m.w9dash_kpi_none_open()} />
					{#if opened?.last_resolved_at}
						<span>{m.w9dash_kpi_last_closed({ ago: formatRelative(opened.last_resolved_at) })}</span>
					{/if}
				{/if}
			</div>
			{#if opened && opened.days.length > 0}
				{@const n = opened.days.length}
				<div class="col-span-2 mt-2.5 min-w-0">
					<svg viewBox={`0 0 ${n * 10 - 3} 26`} preserveAspectRatio="none" class="block h-[26px] w-full" role="img" aria-label={m.w9dash_kpi_opened_label({ days: n })}>
						{#each opened.days as day, i (day.day)}
							{@const h = Math.max(2, (day.count / openedPeak) * 26)}
							<rect
								x={i * 10}
								y={26 - h}
								width="7"
								height={h}
								rx="1.5"
								fill={i === n - 1 ? (firing.length > 0 ? 'var(--color-warning)' : 'var(--color-ink-2)') : 'var(--color-line-strong)'}
							>
								<title>{dayText(day.day)}: {day.count}</title>
							</rect>
						{/each}
					</svg>
					<div class="mt-1 flex justify-between gap-2 text-[0.71875rem] text-ink-3">
						<span>{dayText(opened.days[0].day)}</span>
						<span>{m.w9dash_kpi_today()}</span>
					</div>
				</div>
			{/if}
		</section>
	{/if}
</div>
