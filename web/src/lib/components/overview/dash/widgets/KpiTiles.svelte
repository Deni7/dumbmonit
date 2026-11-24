<script lang="ts">
	/**
	 * The three figures at the head of the overview, in a short row. Each tile
	 * holds its name and its figure on the left and a small chart on the
	 * right, nothing else:
	 *   - monitors up ("9 / 11"), with one bar per monitor grouped by site;
	 *   - availability over 30 days, with its objective as a dashed line;
	 *   - open problems, the rows of "Act now" (monitors down, unreachable or
	 *     misconfigured and firing warning or critical alerts, as the nav
	 *     badge counts them), with the alerts opened per day.
	 * The charts' details (a monitor's name and state, the objective, a day's
	 * count) are in their tooltips and accessible names.
	 *
	 * Monitors and alerts come from the app-wide store; availability and the
	 * days from the widgets store.
	 */
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Skeleton } from '#lib/ui/index.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import { readSky } from '../../sky.js';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { availabilityText, monitorBars } from '../parts/format.js';
	import { STATUS_FILL, type DashStatus } from '../parts/plates.js';
	import { problemRows } from '../parts/problems.js';
	import { groupBySite } from '../parts/site-index.js';
	import { dayText, statusWord } from '../parts/words.js';

	let { class: className = '' }: { class?: string } = $props();

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
	const groups = $derived(groupBySite(monitors, data?.sites ?? [], m.w9dash_common_this_site()));
	const strip = $derived(
		monitorBars(groups.map((g) => ({ key: g.key, name: g.name, statuses: g.monitors.map((r) => r.status) })))
	);
	/** Names for the bars' tooltips, in the strip's order. */
	const stripNames = $derived(groups.flatMap((g) => g.monitors.map((r) => r.name)));

	// --- Availability ---------------------------------------------------------
	const availability = $derived(data?.availability ?? null);
	const availValues = $derived(availability?.days.map((d) => d.pct) ?? []);
	const availLow = $derived.by(() => {
		const target = availability?.slo_target ?? 99.9;
		const floor = target - Math.max(0.1, 100 - target) * 4;
		const seen = availValues.filter((v): v is number => v !== null);
		return Math.max(0, Math.min(floor, ...seen));
	});

	// --- Open problems --------------------------------------------------------
	// The same rows as "Act now": the rules only name the rows, they never add
	// or remove one, so the count does without them.
	const problems = $derived(
		problemRows(readSky({ targets: alertsStore.targets, probes: alertsStore.probes, alerts: alertsStore.alerts, rules: [] }).needsYou)
	);
	const opened = $derived(data?.alerts_opened ?? null);
	const openedPeak = $derived(Math.max(4, ...(opened?.days.map((d) => d.count) ?? [0])));

	const tile = 'flex h-[4.25rem] min-w-0 items-center gap-4 rounded-[var(--radius-card)] border border-line bg-surface px-4';
	const left = 'min-w-0 shrink-0 max-w-[55%]';
	const label = 'm-0 truncate text-sm font-medium text-ink-2';
	const figure = 'font-mono text-2xl leading-none font-medium tracking-[-0.03em] text-ink tabular-nums';
	const unit = 'truncate text-sm text-ink-3';
	const chart = 'h-9 min-w-0 flex-1';
</script>

<div class={`grid grid-cols-1 gap-3 md:grid-cols-3 ${className}`}>
	{#if reading}
		{#each [0, 1, 2] as i (i)}
			<div class={tile} aria-hidden="true">
				<div class="flex flex-col gap-2">
					<Skeleton class="h-4 w-28" />
					<Skeleton class="h-6 w-16" />
				</div>
				<Skeleton class="h-9 min-w-0 flex-1" />
			</div>
		{/each}
	{:else}
		<!-- Monitors up -->
		<section class={tile} aria-label={m.console_kpi_up()}>
			<div class={left}>
				<h2 class={label}>{m.console_kpi_up()}</h2>
				<div class="mt-1.5 flex h-6 items-baseline gap-1.5">
					<span class={figure}>{up}</span>
					<span class={unit}>{m.console_kpi_up_unit({ total: active })}</span>
				</div>
			</div>
			{#if strip.bars.length > 0}
				<svg viewBox={`0 0 ${strip.width} 26`} preserveAspectRatio="none" class={`block ${chart}`} role="img" aria-label={m.w9dash_kpi_bars_label()}>
					{#each strip.bars as bar, i (i)}
						<rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx="1" fill={STATUS_FILL[bar.status as DashStatus]}>
							<title>{stripNames[i]}: {statusWord(bar.status as DashStatus)}</title>
						</rect>
					{/each}
				</svg>
			{/if}
		</section>

		<!-- Availability, 30 days -->
		<section class={tile} aria-label={m.console_kpi_availability()}>
			<div class={left}>
				<h2 class={label}>{m.console_kpi_availability()}</h2>
				<div class="mt-1.5 flex h-6 items-baseline gap-1.5">
					{#if availability}
						<span class={figure}>{availabilityText(availability.avg_30d)}</span>
						{#if availability.avg_30d != null}<span class={unit}>%</span>{/if}
					{:else if widgetsStore.loading}
						<Skeleton class="h-6 w-16" />
					{:else if widgetsStore.error}
						<span class="truncate text-[0.8125rem] font-medium text-warning-ink" role="alert">{m.w9dash_common_error()}</span>
					{:else}
						<span class={figure}>—</span>
					{/if}
				</div>
			</div>
			{#if availability}
				<div class={chart} title={m.w9dash_kpi_avail_note({ target: availability.slo_target })}>
					<Spark
						values={availValues}
						height={36}
						min={availLow}
						max={100}
						ref={availability.slo_target}
						color="var(--color-info)"
						label={m.w9dash_kpi_avail_note({ target: availability.slo_target })}
					/>
				</div>
			{/if}
		</section>

		<!-- Open problems -->
		<section class={tile} aria-label={m.w10hybrid_kpi_problems()}>
			<div class={left}>
				<h2 class={label}>{m.w10hybrid_kpi_problems()}</h2>
				<div class="mt-1.5 flex h-6 items-baseline gap-1.5">
					<span class={figure}>{problems.length}</span>
				</div>
			</div>
			{#if opened && opened.days.length > 0}
				{@const n = opened.days.length}
				<svg viewBox={`0 0 ${n * 10 - 3} 26`} preserveAspectRatio="none" class={`block ${chart}`} role="img" aria-label={m.w9dash_kpi_opened_label({ days: n })}>
					{#each opened.days as day, i (i)}
						{@const h = Math.max(1.5, (day.count / openedPeak) * 26)}
						<rect
							x={i * 10}
							y={26 - h}
							width="7"
							height={h}
							rx="1.5"
							fill={day.count === 0 ? 'var(--color-line-strong)' : i === n - 1 ? 'var(--color-ink-2)' : 'var(--color-ink-3)'}
						>
							<title>{dayText(day.day)}: {day.count}</title>
						</rect>
					{/each}
				</svg>
			{/if}
		</section>
	{/if}
</div>
