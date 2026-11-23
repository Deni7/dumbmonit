<script lang="ts">
	/**
	 * Fleet map: one hexagon per monitor, a block per site, cells in category
	 * order. A cell that is not up carries its glyph (× down, ! degraded,
	 * ‖ paused) as well as its colour; each cell names its monitor on hover.
	 * The figure beside each site name is the count a reader needs: up of watched.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import StatusLegend from '../parts/StatusLegend.svelte';
	import { HEX_SCALE, hexGrid } from '../parts/geometry.js';
	import { STATUS_FILL, STATUS_GLYPH, countStatuses, sortByCategory, type Span } from '../parts/sites.js';
	import { monitorTitle } from '../parts/labels.js';

	let { span = 5 }: { span?: Span } = $props();

	const data = $derived(widgetsStore.data);
	const sites = $derived(
		(data?.sites ?? [])
			.filter((site) => site.monitors.length > 0)
			.map((site) => {
				const monitors = sortByCategory(site.monitors);
				return { key: site.key, name: site.name, monitors, counts: countStatuses(monitors), grid: hexGrid(monitors.length) };
			})
	);
</script>

<WidgetCard {span} title={m.w9layout_hex_title()} sub={m.w9layout_hex_sub()}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={sites.length === 0} emptyText={m.w9layout_fleet_empty()}>
		<div class="@container">
			<div class={`grid gap-3 ${sites.length > 1 ? '@md:grid-cols-2 @xl:grid-cols-3' : ''}`}>
				{#each sites as site (site.key)}
					<div class="min-w-0">
						<h3 class="m-0 mb-1 flex justify-between gap-1.5 text-[0.8125rem] font-semibold text-ink">
							<span class="truncate">{site.name}</span>
							<span class={`shrink-0 font-mono tabular-nums ${site.counts.down ? 'text-warning-ink' : 'text-ink-3'}`}>
								{m.w9layout_count_up_of({ up: site.counts.up, total: site.counts.active })}
							</span>
						</h3>
						<svg
							viewBox={`0 0 ${site.grid.width.toFixed(1)} ${site.grid.height.toFixed(1)}`}
							width={Math.round(site.grid.width * HEX_SCALE)}
							class="block h-auto max-w-full"
							role="img"
							aria-label={m.w9layout_hex_site_aria({ site: site.name, up: site.counts.up, count: site.counts.active, down: site.counts.down })}
						>
							{#each site.monitors as monitor, i (monitor.id)}
								{@const cell = site.grid.cells[i]}
								<g>
									<title>{monitorTitle(monitor)}</title>
									<polygon points={cell.points} fill={STATUS_FILL[monitor.status]} stroke="var(--c-surface)" stroke-width="0.6" />
									{#if monitor.status !== 'up'}
										<text x={cell.cx.toFixed(1)} y={(cell.cy + 3.2).toFixed(1)} text-anchor="middle" font-size="9" font-weight="800" fill="var(--c-surface)">{STATUS_GLYPH[monitor.status]}</text>
									{/if}
								</g>
							{/each}
						</svg>
					</div>
				{/each}
			</div>
		</div>
		<StatusLegend class="mt-3" />
	</DataState>
</WidgetCard>
