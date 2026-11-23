<script lang="ts">
	/**
	 * One card per site: how many monitors answer, its health in a word, the
	 * WAN latency over the range (relay sites), one line per category with a
	 * mark per monitor, the last backup and the next maintenance window. With a
	 * single site (no relay) there is one card, named after the instance.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { Plate } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import Spark from '../Spark.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import Squares from '../parts/Squares.svelte';
	import { countStatuses, groupByCategory, healthOf, lastValue, type Span } from '../parts/sites.js';
	import { HEALTH_TONE, categoryLabel, futureWhen, healthWord, num, pastWhen } from '../parts/labels.js';

	let { span = 4 }: { span?: Span } = $props();

	const data = $derived(widgetsStore.data);
	const loading = $derived(widgetsStore.loading && !data);
	const error = $derived(data ? null : widgetsStore.error);
	const cards = $derived(
		(data?.sites ?? []).map((site) => {
			const counts = countStatuses(site.monitors);
			return {
				site,
				counts,
				health: healthOf(counts),
				wan: lastValue(site.wan_latency_ms),
				groups: groupByCategory(site.monitors).map((group) => ({ ...group, counts: countStatuses(group.monitors) }))
			};
		})
	);
</script>

{#if loading || error || cards.length === 0}
	{#each Array(loading ? 3 : 1) as _, i (i)}
		<WidgetCard {span} title={m.w9layout_sites_title()}>
			<DataState {loading} {error} empty={cards.length === 0} emptyText={m.w9layout_fleet_empty()} rows={5}>
				<span></span>
			</DataState>
		</WidgetCard>
	{/each}
{:else}
	{#each cards as card (card.site.key)}
		<WidgetCard {span} title={card.site.name} sub={m.w9layout_sites_sub({ count: card.counts.total })}>
			<div class="flex flex-col gap-2.5">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<p class="m-0 flex items-baseline gap-1.5">
						<span class="font-mono text-[1.625rem] leading-none font-medium tracking-[-0.02em] text-ink tabular-nums">{card.counts.up}</span>
						<span class="text-ink-3">{m.w9layout_sites_up_of({ count: card.counts.active })}</span>
					</p>
					<Plate tone={HEALTH_TONE[card.health]}>{healthWord(card.health, card.counts.down)}</Plate>
				</div>

				{#if card.site.wan_latency_ms && card.site.wan_latency_ms.length > 1}
					<div class="flex items-center gap-2.5">
						<div class="min-w-0 flex-1"><Spark values={card.site.wan_latency_ms} height={26} /></div>
						{#if card.wan !== null}
							<span class="whitespace-nowrap">
								<b class="font-mono font-semibold text-ink tabular-nums">{m.w9layout_ms({ value: num(card.wan) })}</b>
								<span class="text-ink-3">{m.w9layout_sites_wan()}</span>
							</span>
						{/if}
					</div>
				{/if}

				<ul class="m-0 flex list-none flex-col gap-1.5 p-0">
					{#each card.groups as group (group.category)}
						<li class="grid grid-cols-[72px_minmax(0,1fr)_auto] items-center gap-2 text-[0.8125rem]">
							<span class="truncate text-ink-2">{categoryLabel(group.category)}</span>
							<Squares items={group.monitors} />
							<span class={`text-right font-mono text-xs tabular-nums ${group.counts.down ? 'font-semibold text-warning-ink' : group.counts.degraded ? 'font-semibold text-advisory-ink' : 'text-ink-2'}`}>
								{m.w9layout_count_up_of({ up: group.counts.up, total: group.counts.active })}
							</span>
						</li>
					{/each}
				</ul>

				<dl class="m-0 text-[0.8125rem]">
					<div class="flex items-center gap-2.5 border-t border-line py-2">
						<dt class="flex-1 text-ink-3">{m.w9layout_sites_last_backup()}</dt>
						<dd class="m-0 text-right whitespace-nowrap text-ink">{card.site.last_backup ? pastWhen(card.site.last_backup) : m.w9layout_sites_no_backup()}</dd>
					</div>
					<div class="flex items-center gap-2.5 border-t border-line pt-2">
						<dt class="flex-1 text-ink-3">{m.w9layout_matrix_next_maintenance()}</dt>
						<dd class="m-0 text-right whitespace-nowrap text-ink">{card.site.next_maintenance ? futureWhen(card.site.next_maintenance) : m.w9layout_none_planned()}</dd>
					</div>
				</dl>
			</div>
		</WidgetCard>
	{/each}
{/if}
