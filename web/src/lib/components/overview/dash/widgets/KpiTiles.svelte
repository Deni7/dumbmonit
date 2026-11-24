<script lang="ts">
	/**
	 * The three figures at the head of every dashboard. Each tile reads left to
	 * right: its name, the figure and one status line, then a small chart on
	 * the right (under it when the tile is narrow):
	 *   - monitors up, with one bar per monitor grouped by site;
	 *   - availability over 30 days, with its objective as a dashed line and
	 *     the error budget left;
	 *   - open problems, the rows of the problems table ("Act now"): monitors
	 *     down, unreachable or misconfigured and firing warning or critical
	 *     alerts, as the nav badge and "Needs attention" count them; with the
	 *     alerts opened per day over two weeks.
	 *
	 * Monitors and alerts come from the app-wide store; availability and the
	 * 14 days from the dashboards' store. Not a `WidgetCard`: a row of three
	 * tiles that stacks on a phone.
	 */
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { formatRelative, parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Plate, Skeleton, type Tone } from '#lib/ui/index.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import { readSky } from '../../sky.js';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { availabilityText, budgetTone, monitorBars, pointDelta, signed } from '../parts/format.js';
	import { STATUS_FILL, type DashStatus } from '../parts/plates.js';
	import { problemRows } from '../parts/problems.js';
	import { groupBySite } from '../parts/site-index.js';
	import { dayText, durationText, statusWord } from '../parts/words.js';

	let { class: className = '' }: { class?: string } = $props();

	const data = $derived(widgetsStore.data);
	const reading = $derived(alertsStore.loading && alertsStore.targets.length === 0);

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(timer);
	});

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

	// --- Open problems --------------------------------------------------------
	// The same rows as the problems table: the rules only name the rows, they
	// never add or remove one, so the count does without them.
	const problems = $derived(
		problemRows(readSky({ targets: alertsStore.targets, probes: alertsStore.probes, alerts: alertsStore.alerts, rules: [] }).needsYou)
	);
	const problemTone = $derived.by<Tone>(() => {
		const open = problems.filter((p) => !p.acked && !p.muted);
		if (open.length === 0) return 'muted';
		return open.some((p) => p.row.tone === 'warning') ? 'warning' : 'advisory';
	});
	const oldest = $derived.by(() => {
		let first = Number.POSITIVE_INFINITY;
		for (const problem of problems) {
			const at = parseServerDate(problem.row.since)?.getTime();
			if (at !== undefined && at < first) first = at;
		}
		return Number.isFinite(first) ? durationText(Math.max(0, (now - first) / 1000)) : null;
	});
	const opened = $derived(data?.alerts_opened ?? null);
	const openedPeak = $derived(Math.max(4, ...(opened?.days.map((d) => d.count) ?? [0])));

	const tile = '@container min-w-0 rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3';
	const body = 'flex min-w-0 flex-col gap-2.5 @3xs:flex-row @3xs:items-end @3xs:gap-4';
	const left = 'min-w-0 @3xs:max-w-[58%] @3xs:shrink-0';
	const label = 'm-0 truncate text-sm font-medium text-ink-2';
	const figure = 'font-mono text-[1.75rem] leading-none font-medium tracking-[-0.03em] text-ink tabular-nums';
	const status = 'mt-2 flex h-6 min-w-0 items-center gap-1.5 overflow-hidden text-[0.8125rem] whitespace-nowrap text-ink-2';
	const chart = 'min-w-0 flex-1';
	const caption = 'mt-1 flex min-w-0 justify-between gap-2 text-[0.6875rem] leading-tight text-ink-3';
</script>

<div class={`grid grid-cols-1 gap-3 md:grid-cols-3 ${className}`}>
	{#if reading}
		{#each [0, 1, 2] as i (i)}
			<div class={tile} aria-hidden="true">
				<Skeleton class="h-4 w-28" />
				<Skeleton class="mt-2 h-7 w-20" />
				<Skeleton class="mt-2 h-5 w-full" />
			</div>
		{/each}
	{:else}
		<!-- Monitors up -->
		<section class={tile} aria-label={m.console_kpi_up()}>
			<div class={body}>
				<div class={left}>
					<h2 class={label}>{m.console_kpi_up()}</h2>
					<p class="m-0 mt-1.5 flex items-baseline gap-1.5">
						<span class={figure}>{up}</span>
						<span class="truncate text-sm text-ink-3">{m.console_kpi_up_unit({ total: active })}</span>
					</p>
					<div class={status}>
						{#if down > 0}
							<Plate tone="warning" label={m.w9dash_kpi_down({ count: down })} class="shrink-0" />
						{:else if active > 0}
							<Plate tone="signal" label={m.w9dash_kpi_all_up()} class="shrink-0" />
						{/if}
						{#if paused > 0}<span class="min-w-0 truncate">{m.w9dash_kpi_paused({ count: paused })}</span>{/if}
					</div>
				</div>
				{#if strip.bars.length > 0}
					<div class={chart}>
						<svg viewBox={`0 0 ${strip.width} 26`} preserveAspectRatio="none" class="block h-10 w-full" role="img" aria-label={m.w9dash_kpi_bars_label()}>
							{#each strip.bars as bar, i (i)}
								<rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx="1" fill={STATUS_FILL[bar.status as DashStatus]}>
									<title>{stripNames[i]}: {statusWord(bar.status as DashStatus)}</title>
								</rect>
							{/each}
						</svg>
						{#if strip.labels.length > 1}
							<div class={caption}>
								{#each strip.labels as site (site.key)}
									<span class="min-w-0 truncate" style={`flex:${site.share} 1 0`}>{site.name}</span>
								{/each}
							</div>
						{/if}
					</div>
				{/if}
			</div>
		</section>

		<!-- Availability, 30 days -->
		<section class={tile} aria-label={m.console_kpi_availability()}>
			<div class={body}>
				<div class={left}>
					<h2 class={label}>{m.console_kpi_availability()}</h2>
					<p class="m-0 mt-1.5 flex items-baseline gap-1.5">
						<span class={figure}>{availabilityText(availability?.avg_30d)}</span>
						{#if availability?.avg_30d != null}<span class="text-sm text-ink-3">%</span>{/if}
					</p>
					<div class={status}>
						{#if availability}
							{#if budget !== null && budgetState !== 'ok'}
								<Plate
									tone="advisory"
									label={budgetState === 'spent' ? m.w10hybrid_kpi_budget_spent() : m.w10hybrid_kpi_budget_low({ pct: Math.round(budget) })}
									class="shrink-0"
								/>
							{/if}
							{#if availDelta !== null}
								{@const text = m.w9dash_kpi_avail_delta({ delta: signed(availDelta) })}
								<span class={`min-w-0 truncate font-medium ${availDelta > 0 ? 'text-signal-ink' : availDelta < 0 ? 'text-advisory-ink' : ''}`} title={text}>{text}</span>
							{/if}
						{:else if widgetsStore.error}
							<span class="truncate text-advisory-ink" role="alert">{m.w9dash_common_error()}</span>
						{:else if widgetsStore.loading}
							<Skeleton class="h-4 w-32" />
						{:else}
							<span class="truncate">{m.console_kpi_none()}</span>
						{/if}
					</div>
				</div>
				{#if availability}
					<div class={chart}>
						<div class="h-10"><Spark values={availValues} height={40} min={availLow} max={100} ref={availability.slo_target} color="var(--color-info)" /></div>
						<div class={caption}>
							<span class="min-w-0 truncate">{m.w9dash_kpi_avail_note({ target: availability.slo_target })}</span>
							{#if budget !== null && budgetState === 'ok'}
								<span class="shrink-0">{m.w10hybrid_kpi_budget_left({ pct: Math.round(budget) })}</span>
							{/if}
						</div>
					</div>
				{/if}
			</div>
		</section>

		<!-- Open problems -->
		<section class={tile} aria-label={m.w10hybrid_kpi_problems()}>
			<div class={body}>
				<div class={left}>
					<h2 class={label}>{m.w10hybrid_kpi_problems()}</h2>
					<p class="m-0 mt-1.5 flex items-baseline gap-1.5">
						<span class={figure}>{problems.length}</span>
					</p>
					<div class={status}>
						{#if problems.length > 0}
							{#if oldest}<Plate tone={problemTone} label={m.w10hybrid_kpi_oldest({ age: oldest })} class="shrink-0" />{/if}
						{:else}
							<Plate tone="signal" label={m.w9dash_kpi_none_open()} class="shrink-0" />
							{#if opened?.last_resolved_at}
								<span class="min-w-0 truncate">{m.w9dash_kpi_last_closed({ ago: formatRelative(opened.last_resolved_at) })}</span>
							{/if}
						{/if}
					</div>
				</div>
				{#if opened && opened.days.length > 0}
					{@const n = opened.days.length}
					{@const week = m.w9dash_kpi_opened_week({ count: opened.this_week, previous: opened.last_week })}
					<div class={chart}>
						<svg viewBox={`0 0 ${n * 10 - 3} 26`} preserveAspectRatio="none" class="block h-10 w-full" role="img" aria-label={m.w9dash_kpi_opened_label({ days: n })}>
							{#each opened.days as day, i (day.day)}
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
						<div class={caption}>
							<span class="min-w-0 truncate" title={week}>{week}</span>
						</div>
					</div>
				{/if}
			</div>
		</section>
	{/if}
</div>
