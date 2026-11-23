<script lang="ts">
	/**
	 * Health by site and category: one row per site, one column per category
	 * in use, one mark per monitor (its shape carries its status), the count
	 * under the marks, then the WAN latency over the range and the next
	 * maintenance window. Scrolls sideways inside the card on a phone.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { consoleState } from '#lib/stores/console.svelte.js';
	import { Plate } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import StatusLegend from '../parts/StatusLegend.svelte';
	import Squares from '../parts/Squares.svelte';
	import { countStatuses, healthOf, lastValue, usedCategories, type Span } from '../parts/sites.js';
	import { HEALTH_TONE, categoryLabel, futureWhen, healthWord, num } from '../parts/labels.js';

	let { span = 8 }: { span?: Span } = $props();

	const data = $derived(widgetsStore.data);
	const sites = $derived((data?.sites ?? []).filter((site) => site.monitors.length > 0));
	const columns = $derived(usedCategories(sites));
	const rows = $derived(
		sites.map((site) => {
			const counts = countStatuses(site.monitors);
			return {
				site,
				counts,
				health: healthOf(counts),
				wan: lastValue(site.wan_latency_ms),
				cells: columns.map((category) => {
					const monitors = site.monitors.filter((monitor) => monitor.category === category);
					return { category, monitors, counts: countStatuses(monitors) };
				})
			};
		})
	);

	function cellText(c: ReturnType<typeof countStatuses>): string {
		if (c.down) return m.w9layout_matrix_cell_down({ up: c.up, total: c.active, count: c.down });
		if (c.degraded) return m.w9layout_matrix_cell_degraded({ up: c.up, total: c.active, count: c.degraded });
		if (c.paused) return m.w9layout_matrix_cell_paused({ up: c.up, total: c.active, count: c.paused });
		return m.w9layout_count_up_of({ up: c.up, total: c.active });
	}
</script>

<WidgetCard {span} flush title={m.w9layout_matrix_title()} sub={m.w9layout_matrix_sub()}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={rows.length === 0} emptyText={m.w9layout_fleet_empty()} class="px-4 pb-4">
		<div class="overflow-x-auto">
			<table class="w-full min-w-[720px] border-collapse text-[0.8125rem]">
				<thead>
					<tr class="border-b border-line text-left text-xs font-medium whitespace-nowrap text-ink-3">
						<th scope="col" class="py-1.5 pr-2.5 pl-4 font-medium">{m.w9layout_matrix_site()}</th>
						{#each columns as category (category)}
							<th scope="col" class="px-2.5 py-1.5 font-medium">{categoryLabel(category)}</th>
						{/each}
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_matrix_wan({ range: consoleState.range })}</th>
						<th scope="col" class="py-1.5 pr-4 pl-2.5 font-medium">{m.w9layout_matrix_next_maintenance()}</th>
					</tr>
				</thead>
				<tbody>
					{#each rows as row (row.site.key)}
						<tr class="border-b border-line align-middle last:border-b-0">
							<th scope="row" class="py-2 pr-2.5 pl-4 text-left font-normal">
								<span class="block font-semibold whitespace-nowrap text-ink">{row.site.name}</span>
								<Plate tone={HEALTH_TONE[row.health]} class="mt-1">{healthWord(row.health, row.counts.down)}</Plate>
							</th>
							{#each row.cells as cell (cell.category)}
								<td class="px-2.5 py-2">
									{#if cell.monitors.length}
										<div class="flex min-w-[84px] flex-col gap-1">
											<Squares items={cell.monitors} class="max-w-[110px]" />
											<span class={`text-xs ${cell.counts.down ? 'font-semibold text-warning-ink' : cell.counts.degraded ? 'font-semibold text-advisory-ink' : 'text-ink-2'}`}>{cellText(cell.counts)}</span>
										</div>
									{:else}
										<span class="text-xs text-ink-3">{m.w9layout_matrix_none()}</span>
									{/if}
								</td>
							{/each}
							<td class="min-w-[130px] px-2.5 py-2">
								{#if row.site.wan_latency_ms && row.site.wan_latency_ms.length > 1}
									<div class="flex items-center gap-2">
										<div class="w-20 shrink-0"><Spark values={row.site.wan_latency_ms} height={22} /></div>
										{#if row.wan !== null}
											<b class="font-mono font-semibold whitespace-nowrap text-ink tabular-nums">{m.w9layout_ms({ value: num(row.wan) })}</b>
										{/if}
									</div>
								{:else}
									<span class="text-xs text-ink-3">{row.site.wan_latency_ms === null ? m.w9layout_matrix_wan_local() : m.w9layout_matrix_wan_none()}</span>
								{/if}
							</td>
							<td class="py-2 pr-4 pl-2.5 whitespace-nowrap">
								{#if row.site.next_maintenance}
									<Plate tone="info">{futureWhen(row.site.next_maintenance)}</Plate>
								{:else}
									<span class="text-xs text-ink-3">{m.w9layout_none_planned()}</span>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<StatusLegend shape="square" class="border-t border-line px-4 py-3" />
	</DataState>
</WidgetCard>
