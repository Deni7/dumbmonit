<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Alerts and response over the last 30 days: how many alerts, how fast
	 * someone acknowledged them (MTTA) and how fast they were resolved (MTTR),
	 * each against the 30 days before; the alerts per day stacked by severity;
	 * the share acknowledged within 15 minutes; and the noisiest rule, with a
	 * way to tune it. From the dashboards' store.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { ErrorNotice, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { deltaOf, stackBars, type Delta } from '../parts/format.js';
	import { SEVERITY_FILL } from '../parts/plates.js';
	import { dayText, durationText } from '../parts/words.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 4 }: { span?: Span } = $props();

	const W = 300;
	const H = 56;

	const stats = $derived(widgetsStore.data?.alert_stats ?? null);
	const chart = $derived(stats ? stackBars(stats.days, { width: W, height: H, gap: 2 }) : null);

	const figures = $derived.by(() => {
		if (!stats) return [];
		const total = deltaOf(stats.total, stats.previous_total, { lowerIsBetter: true });
		const mtta = deltaOf(stats.mtta_s, stats.previous_mtta_s, { lowerIsBetter: true, flatWithin: 30 });
		const mttr = deltaOf(stats.mttr_s, stats.previous_mttr_s, { lowerIsBetter: true, flatWithin: 60 });
		return [
			{
				key: 'alerts',
				label: m.w9dash_response_alerts(),
				value: String(stats.total),
				delta: total,
				before:
					total && total.pct !== null
						? m.w9dash_response_before_pct({ previous: stats.previous_total, pct: `${total.pct > 0 ? '+' : total.pct < 0 ? '−' : ''}${Math.abs(Math.round(total.pct))}` })
						: m.w9dash_response_before({ previous: String(stats.previous_total) })
			},
			{
				key: 'mtta',
				label: m.w9dash_response_mtta(),
				value: stats.mtta_s === null ? '—' : durationText(stats.mtta_s),
				delta: mtta,
				before: stats.previous_mtta_s === null ? '' : m.w9dash_response_before({ previous: durationText(stats.previous_mtta_s) })
			},
			{
				key: 'mttr',
				label: m.w9dash_response_mttr(),
				value: stats.mttr_s === null ? '—' : durationText(stats.mttr_s),
				delta: mttr,
				before: stats.previous_mttr_s === null ? '' : m.w9dash_response_before({ previous: durationText(stats.previous_mttr_s) })
			}
		];
	});

	function deltaTone(delta: Delta | null): string {
		if (!delta || delta.better === null) return 'text-ink-3';
		return delta.better ? 'text-signal-ink' : 'text-advisory-ink';
	}

	const SEVERITIES = $derived([
		{ key: 'critical' as const, label: m.w9dash_common_sev_critical() },
		{ key: 'warning' as const, label: m.w9dash_common_sev_warning() },
		{ key: 'info' as const, label: m.alerts_severity_info() }
	]);
	const tuneHref = $derived(auth.canOperate ? '/alerts?tab=noise' : '/alerts?tab=rules');
</script>

<WidgetCard title={m.w9dash_response_title()} sub={m.w9dash_common_last_30_days()} {span}>
	{#if !stats && widgetsStore.loading}
		<div class="flex flex-col gap-2.5" aria-hidden="true">
			<Skeleton class="h-14 w-full" />
			<Skeleton class="h-16 w-full" />
		</div>
	{:else if !stats}
		<ErrorNotice error={widgetsStore.error} title={m.w9dash_common_error()} compact onretry={() => void widgetsStore.refresh()} />
	{:else}
		<div class="mb-2.5 grid grid-cols-3 gap-2.5 max-[22rem]:grid-cols-1">
			{#each figures as figure (figure.key)}
				<div class="min-w-0 rounded-[10px] bg-surface-2 px-2.5 py-2">
					<p class="m-0 truncate text-xs text-ink-2">{figure.label}</p>
					<p class="m-0 truncate font-mono text-[1.375rem] leading-tight font-medium tracking-[-0.02em] text-ink tabular-nums">{figure.value}</p>
					{#if figure.before}<p class={`m-0 truncate text-[0.71875rem] ${deltaTone(figure.delta)}`}>{figure.before}</p>{/if}
				</div>
			{/each}
		</div>

		{#if chart}
			<svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" class="block h-16 w-full" role="img" aria-label={m.w9dash_response_chart_label({ days: stats.days.length })}>
				{#each [H / 3, (H * 2) / 3] as y (y)}
					<line x1="0" x2={W} y1={y} y2={y} stroke="var(--color-line)" vector-effect="non-scaling-stroke" />
				{/each}
				{#each chart.empty as tick (tick.x)}
					<rect x={tick.x} y={H - 2} width={tick.w} height="2" fill="var(--color-line-strong)" />
				{/each}
				{#each chart.rects as rect (`${rect.x}-${rect.severity}`)}
					<rect x={rect.x} y={rect.y} width={rect.w} height={Math.max(0.5, rect.h - 0.8)} rx="1.2" fill={SEVERITY_FILL[rect.severity]} />
				{/each}
			</svg>
			{#if stats.days.length > 0}
				<div class="mt-1 flex justify-between gap-2 text-[0.71875rem] text-ink-3">
					<span>{dayText(stats.days[0].day)}</span>
					{#if stats.days.length > 14}<span class="max-[22rem]:hidden">{dayText(stats.days[Math.floor(stats.days.length / 2)].day)}</span>{/if}
					<span>{m.w9dash_kpi_today()}</span>
				</div>
			{/if}
			<div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-ink-2">
				{#each SEVERITIES as severity (severity.key)}
					<span class="inline-flex items-center gap-1.5">
						<span aria-hidden="true" class="size-2.5 rounded-[3px]" style={`background:${SEVERITY_FILL[severity.key]}`}></span>{severity.label}
					</span>
				{/each}
			</div>
		{/if}

		<ul class="m-0 mt-2 list-none p-0 text-sm">
			{#if stats.acked_within_15m_pct !== null}
				<li class="flex items-center gap-2.5 border-t border-line py-2">
					<span class="min-w-0 flex-1 text-ink-2">{m.w9dash_response_acked_fast()}</span>
					<b class="font-mono font-semibold text-ink tabular-nums">{Math.round(stats.acked_within_15m_pct)}%</b>
				</li>
			{/if}
			{#if stats.noisiest}
				{@const noisy = stats.noisiest}
				<li class="flex items-center gap-2.5 border-t border-line py-1">
					<span class="min-w-0 flex-1 truncate text-ink-2">
						{m.w9dash_response_noisiest()}
						{#if noisy.target_id !== null}
							<a href={withBase(`/targets/${noisy.target_id}`)} class="font-semibold text-ink hover:underline">{noisy.target}</a>
						{:else}
							<b class="font-semibold text-ink">{noisy.target}</b>
						{/if}
						<span class="text-ink-3">{m.w9dash_response_noisiest_rule({ rule: noisy.rule, count: noisy.count })}</span>
					</span>
					<a href={withBase(tuneHref)} class="inline-flex min-h-10 shrink-0 items-center text-[0.8125rem] font-medium text-ink-2 hover:text-ink hover:underline">{m.w9dash_response_tune()}</a>
				</li>
			{/if}
		</ul>
	{/if}
</WidgetCard>
