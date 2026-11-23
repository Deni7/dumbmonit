<script lang="ts">
	/**
	 * Top consumers: the five monitors using the most CPU, memory, disk or
	 * network right now, each with a bar, the figure and its trend over the
	 * range. With `metric` the card shows that one list and no tabs (the NOC
	 * layout puts four side by side); without, small tabs switch the list.
	 * A percentage at 85 turns the bar amber, at 90 red; the figure says it too.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { consoleState } from '#lib/stores/console.svelte.js';
	import type { TopConsumers, TopEntry } from '#lib/api/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { barWidth, loadOf, type Load, type Span } from '../parts/sites.js';
	import { num } from '../parts/labels.js';

	type Metric = keyof TopConsumers;

	let { span = 4, metric }: { span?: Span; metric?: Metric } = $props();

	const METRICS: Metric[] = ['cpu', 'mem', 'disk', 'net'];
	let picked = $state<Metric>('cpu');
	const shown = $derived<Metric>(metric ?? picked);

	const data = $derived(widgetsStore.data);
	const entries = $derived<TopEntry[]>((data?.top[shown] ?? []).slice(0, 5));
	const max = $derived(Math.max(1, ...entries.map((entry) => entry.value)));

	function metricLabel(key: Metric): string {
		switch (key) {
			case 'cpu':
				return m.w9layout_top_cpu();
			case 'mem':
				return m.w9layout_top_mem();
			case 'disk':
				return m.w9layout_top_disk();
			case 'net':
				return m.w9layout_top_net();
		}
	}

	function fixedTitle(key: Metric): string {
		switch (key) {
			case 'cpu':
				return m.w9layout_top_title_cpu();
			case 'mem':
				return m.w9layout_top_title_mem();
			case 'disk':
				return m.w9layout_top_title_disk();
			case 'net':
				return m.w9layout_top_title_net();
		}
	}

	function value(entry: TopEntry): string {
		if (entry.unit === '%') return m.w9layout_pct({ value: num(entry.value) });
		return m.w9layout_rate({ value: num(entry.value, entry.value >= 100 ? 0 : 1), unit: entry.unit });
	}

	function meta(entry: TopEntry): string {
		const days = entry.forecast_days;
		if (shown === 'disk' && days !== null && days !== undefined && Number.isFinite(days)) {
			return m.w9layout_top_forecast({ site: entry.site, count: Math.max(0, Math.round(days)) });
		}
		return entry.site;
	}

	const BAR: Record<Load, string> = { critical: 'bg-warning', high: 'bg-advisory', normal: 'bg-ink-3' };
	const INK: Record<Load, string> = { critical: 'text-warning-ink', high: 'text-advisory-ink', normal: 'text-ink' };
</script>

{#snippet tabs()}
	<div class="inline-flex gap-0.5 rounded-lg bg-surface-2 p-0.5" role="group" aria-label={m.w9layout_top_metric()}>
		{#each METRICS as key (key)}
			<button
				type="button"
				class={`relative min-h-8 rounded-md px-2 text-xs font-medium before:absolute before:inset-x-0 before:-inset-y-1 before:content-[''] ${picked === key ? 'bg-surface text-ink shadow-lift' : 'text-ink-2 hover:text-ink'}`}
				aria-pressed={picked === key}
				onclick={() => (picked = key)}
			>
				{metricLabel(key)}
			</button>
		{/each}
	</div>
{/snippet}

<WidgetCard
	{span}
	title={metric ? fixedTitle(metric) : m.w9layout_top_title()}
	sub={shown === 'net' ? m.w9layout_top_sub_rate({ range: consoleState.range }) : m.w9layout_top_sub_pct({ range: consoleState.range })}
	right={metric ? undefined : tabs}
>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={entries.length === 0} emptyText={m.w9layout_top_empty()} rows={5}>
		<ul class="@container m-0 list-none p-0">
			{#each entries as entry (`${entry.target_id}:${entry.label}`)}
				{@const load = loadOf(entry.value, entry.unit)}
				<li class="border-t border-line first:border-t-0">
					<a
						href={withBase(`/targets/${entry.target_id}`)}
						class="grid min-h-10 grid-cols-[minmax(0,1fr)_auto] items-center gap-2.5 py-1.5 hover:bg-surface-2 @min-[340px]:grid-cols-[minmax(0,1.2fr)_minmax(60px,1fr)_auto_70px]"
					>
						<span class="min-w-0">
							<b class="block truncate font-semibold text-ink">{entry.label}</b>
							<span class="block truncate text-xs text-ink-2">{meta(entry)}</span>
						</span>
						<span class="relative hidden h-2 min-w-[60px] overflow-hidden rounded bg-ghost @min-[340px]:block" aria-hidden="true">
							<i class={`absolute inset-y-0 left-0 block rounded ${BAR[load]}`} style={`width:${barWidth(entry.value, entry.unit, max)}%`}></i>
						</span>
						<span class={`text-right font-mono font-semibold whitespace-nowrap tabular-nums ${INK[load]}`}>{value(entry)}</span>
						<span class="hidden @min-[340px]:block"><Spark values={entry.series} height={22} area={false} /></span>
					</a>
				</li>
			{/each}
		</ul>
	</DataState>
</WidgetCard>
