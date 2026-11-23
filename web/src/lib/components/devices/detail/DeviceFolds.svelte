<script lang="ts">
	/** Instruments of a device monitor: essentials, containers, interfaces and the searchable long tail, each folded. */
	import { ChevronDown, Search } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Target } from '#lib/api/index.js';
	import type { thresholdsFor } from '#lib/chart-context.js';
	import Chart, { type ChartEvent, type Serie } from '#lib/components/Chart.svelte';
	import CatalogViews from '#lib/components/devices/CatalogViews.svelte';
	import type { CatalogLayout } from '#lib/components/devices/catalog.js';
	import FoldRow from '#lib/components/devices/FoldRow.svelte';
	import FoldSection from '#lib/components/devices/FoldSection.svelte';
	import DockerPanel from '#lib/components/devices/docker/DockerPanel.svelte';
	import { isDockerDevice } from '#lib/components/devices/agent/services.js';
	import {
		formatRate,
		isVirtualInterface,
		lastValue,
		steadyValue,
		type DeviceMetricGroup,
		type MetricRow,
		type MetricSections
	} from '#lib/components/devices/metrics.js';
	import { Button, EmptyState, Plate } from '#lib/ui/index.js';

	interface Props {
		target: Target;
		layout: CatalogLayout | null;
		sections: MetricSections;
		/** The fold under the charts: everything else, or every family when the catalogue picked the charts. */
		tail: DeviceMetricGroup[];
		groups: DeviceMetricGroup[];
		loadingMetrics: boolean;
		monitorOn: boolean;
		probing: boolean;
		probe: () => void | Promise<void>;
		syncKey: string;
		chartEvents: ChartEvent[];
		thresholdsOf: (metric: string, unit: string, scale?: number) => ReturnType<typeof thresholdsFor>;
		previousOf: (metric: string) => (window: { start: number; end: number }) => Promise<Serie[]>;
		/** Interface rows unfolded by the user. */
		openInterfaces: Record<string, boolean>;
		/** The search over the long tail. */
		otherQuery: string;
		showSteady: boolean;
	}

	let {
		target,
		layout,
		sections,
		tail,
		groups,
		loadingMetrics,
		monitorOn,
		probing,
		probe,
		syncKey,
		chartEvents,
		thresholdsOf,
		previousOf,
		openInterfaces = $bindable(),
		otherQuery = $bindable(),
		showSteady = $bindable()
	}: Props = $props();

	const otherShown = $derived.by(() => {
		const q = otherQuery.trim().toLowerCase();
		if (!q) return tail;
		return tail.filter(
			(g) => g.name.toLowerCase().includes(q) || g.title.toLowerCase().includes(q)
		);
	});

	/** Flat families fold into one line; only the ones that move get a chart. */
	const otherMoving = $derived(otherShown.filter((g) => steadyValue(g) === undefined));
	const otherSteady = $derived(otherShown.filter((g) => steadyValue(g) !== undefined));

	/** "↓ 13 kB/s · ↑ 505 kB/s" for the folded interface line. */
	function interfaceLine(row: MetricRow): string {
		const octets = row.charts.find((c) => c.name.endsWith('_octets'));
		if (!octets) return '';
		const of = (dir: string) => lastValue(octets.series.find((s) => s.label === dir));
		return `↓ ${formatRate(of('in'), 'B/s')} · ↑ ${formatRate(of('out'), 'B/s')}`;
	}
	const physicalCount = $derived(sections.interfaces.filter((r) => !isVirtualInterface(r.key)).length);
</script>

	<div class="flex flex-col gap-4">
		<!-- Essentials: the handful of charts that answer "is it fine?" — the ones the
		     kind's catalogue declares, or the generic cpu / memory / disk / network -->
		{#if layout}
			<FoldSection kind="essentials" title={m.devices_target_essentials()} defaultOpen summary={layout.charts.length === 1 ? m.devices_target_charts_one({ count: 1 }) : m.devices_target_charts_other({ count: layout.charts.length })} class="rise-in">
				<CatalogViews {layout} />
			</FoldSection>
		{:else if sections.essentials.length > 0}
			<FoldSection kind="essentials" title={m.devices_target_essentials()} defaultOpen summary={sections.essentials.length === 1 ? m.devices_target_charts_one({ count: 1 }) : m.devices_target_charts_other({ count: sections.essentials.length })} class="rise-in">
				<div class="grid gap-4 px-4 py-4 sm:px-5 lg:grid-cols-2">
					{#each sections.essentials as group, i (group.name)}
						<div class="rise-in" style="--rise-delay: {Math.min(i, 8) * 40}ms">
							<div class="flex items-center justify-between gap-2 pb-1">
								<h3 class="text-sm font-semibold text-ink">{group.title}</h3>
								{#if group.unit}<span class="label-tape label-tape-unit">{group.unit}</span>{/if}
							</div>
							<Chart series={group.series} unit={group.unit} height={200} {syncKey} events={chartEvents} thresholds={thresholdsOf(group.name, group.unit)} loadPrevious={previousOf(group.name)} />
						</div>
					{/each}
				</div>
			</FoldSection>
		{/if}

		<!-- Containers: the Docker panel folds, one row per container -->
		{#if isDockerDevice(target)}
			<DockerPanel {target} metrics={sections.containers} {loadingMetrics} />
		{/if}

		<!-- Interfaces: one row each, physical ones first -->
		{#if sections.interfaces.length > 0}
			<FoldSection
				kind="interfaces"
				title={m.devices_target_interfaces()}
				summary={physicalCount < sections.interfaces.length ? m.devices_target_interfaces_summary({ count: sections.interfaces.length, physical: physicalCount }) : `${sections.interfaces.length}`}
				class="rise-in"
			>
				<ul class="divide-y divide-line">
					{#each sections.interfaces as row, i (row.key)}
						<FoldRow id={`interface-${i}`} bind:open={openInterfaces[row.key]}>
							{#snippet header()}
								<span class="flex min-w-0 items-center gap-2">
									<span class="truncate font-semibold text-ink">{row.key}</span>
									{#if isVirtualInterface(row.key)}
										<Plate tone="ghost" bare label={m.devices_target_virtual()} />
									{/if}
								</span>
								<span class="tnum truncate text-[0.8125rem] text-ink-2">{interfaceLine(row)}</span>
							{/snippet}
							<div class="grid gap-3 lg:grid-cols-2">
								{#each row.charts as chart (chart.name)}
									<div class="rounded-lg border border-line px-3 pt-2 pb-1">
										<div class="flex items-center justify-between gap-2">
											<span class="text-sm font-semibold text-ink">{chart.title}</span>
											{#if chart.unit}<span class="label-tape label-tape-unit">{chart.unit}</span>{/if}
										</div>
										<Chart series={chart.series} unit={chart.unit} height={140} pair={chart.series.length === 2 && chart.series[0].label === m.devices_metrics_in() && chart.series[1].label === m.devices_metrics_out()} {syncKey} events={chartEvents} thresholds={thresholdsOf(chart.name, chart.unit)} />
									</div>
								{/each}
							</div>
						</FoldRow>
					{/each}
				</ul>
			</FoldSection>
		{/if}

		<!-- The long tail, alphabetical, searchable -->
		{#if tail.length > 0}
			<FoldSection
				kind={layout ? 'all-measurements' : 'other'}
				title={layout ? m.devices_target_all_measurements() : m.devices_target_everything_else()}
				defaultOpen={!layout && sections.essentials.length === 0}
				summary={otherQuery.trim() ? m.devices_target_families_filtered({ shown: otherShown.length, total: tail.length }) : tail.length === 1 ? m.devices_target_families_one({ count: 1 }) : m.devices_target_families_other({ count: tail.length })}
				class="rise-in"
			>
				{#snippet aside()}
					<label class="relative block w-full sm:w-56">
						<span class="sr-only">{m.devices_target_search_families()}</span>
						<Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
						<input class="input h-10 w-full text-sm" style="padding-left: 2rem" type="search" placeholder={m.devices_target_filter_families()} bind:value={otherQuery} />
					</label>
				{/snippet}
				{#if otherShown.length === 0}
					<p class="px-5 py-4 text-sm text-ink-2">{m.devices_target_no_family({ query: otherQuery })}</p>
				{:else}
					{#if otherMoving.length > 0}
						<div class="grid gap-4 px-4 py-4 sm:px-5 lg:grid-cols-2">
							{#each otherMoving as group (group.name)}
								<div>
									<div class="flex items-center justify-between gap-2 pb-1">
										<h3 class="text-sm font-semibold text-ink" title={group.name}>{group.title}</h3>
										{#if group.unit}<span class="label-tape label-tape-unit">{group.unit}</span>{/if}
									</div>
									<Chart series={group.series} unit={group.unit} height={180} {syncKey} events={chartEvents} thresholds={thresholdsOf(group.name, group.unit)} loadPrevious={previousOf(group.name)} />
								</div>
							{/each}
						</div>
					{/if}
					{#if otherSteady.length > 0}
						<div class={otherMoving.length > 0 ? 'border-t border-line' : ''}>
							<button
								type="button"
								class="flex min-h-10 w-full items-center gap-2 px-4 py-2 text-left text-sm font-medium text-ink-2 hover:text-ink sm:px-5"
								aria-expanded={showSteady || otherQuery.trim() !== ''}
								aria-controls="steady-families"
								onclick={() => (showSteady = !showSteady)}
							>
								<ChevronDown class={`size-4 transition-transform ${showSteady || otherQuery.trim() ? 'rotate-180' : ''}`} aria-hidden="true" />
								{m.devices_target_steady({ count: otherSteady.length })}
							</button>
							{#if showSteady || otherQuery.trim()}
								<ul id="steady-families" class="grid gap-x-6 px-4 pb-3 sm:px-5 md:grid-cols-2 lg:grid-cols-3">
									{#each otherSteady as group (group.name)}
										<li class="flex items-baseline justify-between gap-3 border-t border-line py-1.5 text-[0.8125rem]" title={group.name}>
											<span class="min-w-0 truncate text-ink-2">{group.title}</span>
											<span class="tnum shrink-0 font-medium text-ink">{Number(Number(steadyValue(group)).toPrecision(4))}{group.unit && group.unit !== '0/1' ? ` ${group.unit}` : ''}</span>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					{/if}
				{/if}
			</FoldSection>
		{/if}

		{#if groups.length === 0}
			<EmptyState title={m.devices_target_no_data_title()} description={target.enabled ? m.devices_target_no_data_desc() : m.devices_target_disabled_empty()}>
				{#snippet action()}
					{#if monitorOn}
						<Button variant="secondary" onclick={probe} loading={probing}>{m.devices_target_probe_now()}</Button>
					{/if}
				{/snippet}
			</EmptyState>
		{/if}
	</div>
