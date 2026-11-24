<script lang="ts">
	/**
	 * Fleet by site, compact (Hybrid): per site, how many monitors answer, its
	 * health in a word, the WAN latency of a relay site, then one line per
	 * category with a mark per monitor (its shape carries the status) and the
	 * count up. Sites and categories come from the dashboards' store; each
	 * monitor's state is the app-wide one when known, so the figures agree
	 * with "Monitors up" and the problems beside them (the server's view can
	 * be a poll behind). With a single site the count and the health word are
	 * left to "Monitors up". `fill`: the card takes its parent's height and
	 * scrolls inside.
	 */
	import type { WidgetStatus } from '#lib/api/index.js';
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Plate } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import Squares from '../parts/Squares.svelte';
	import { countStatuses, groupByCategory, healthOf, lastValue, type Span } from '../parts/sites.js';
	import { HEALTH_TONE, categoryLabel, healthWord, num } from '../parts/labels.js';

	let { span = 4, fill = false, class: cls = '' }: { span?: Span; fill?: boolean; class?: string } = $props();

	const data = $derived(widgetsStore.data);

	/** The app-wide state of each monitor, in the widgets' four words; "waiting" keeps the server's. */
	const live = $derived.by(() => {
		const counts = countAlertsByTarget(alertsStore.alerts);
		const out = new Map<number, WidgetStatus>();
		for (const target of alertsStore.targets) {
			const status = consoleStatus(target, alertsStore.probes.get(target.id), counts.get(target.id));
			if (status === 'waiting') continue;
			out.set(target.id, status === 'config' ? 'degraded' : status);
		}
		return out;
	});

	const sites = $derived(
		(data?.sites ?? []).map((site) => {
			const monitors = site.monitors.map((monitor) => ({ ...monitor, status: live.get(monitor.id) ?? monitor.status }));
			const counts = countStatuses(monitors);
			return {
				key: site.key,
				name: site.name,
				counts,
				health: healthOf(counts),
				wan: lastValue(site.wan_latency_ms),
				groups: groupByCategory(monitors).map((group) => ({ ...group, counts: countStatuses(group.monitors) }))
			};
		})
	);
	// One site: "Monitors up" above already gives its count and health; only the categories are new.
	const several = $derived(sites.length > 1);
</script>

<WidgetCard {span} {fill} flush title={m.w9layout_sun_title()} class={cls}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={sites.length === 0} emptyText={m.w9layout_fleet_empty()} class="px-4 pb-3" rows={4}>
		{#each sites as site (site.key)}
			<div class="border-t border-line px-4 pt-2.5 pb-3">
				<div class="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
					<h3 class="m-0 min-w-0 truncate text-sm font-semibold text-ink">{site.name}</h3>
					{#if several}
						<span class="text-[0.8125rem] whitespace-nowrap text-ink-2 tabular-nums">{site.counts.up} {m.w9layout_sites_up_of({ count: site.counts.active })}</span>
					{/if}
					{#if site.wan !== null}
						<span class="text-[0.8125rem] whitespace-nowrap text-ink-3">{m.w9layout_sites_wan()} {m.w9layout_ms({ value: num(site.wan) })}</span>
					{/if}
					{#if several}
						<Plate tone={HEALTH_TONE[site.health]} label={healthWord(site.health, site.counts.down)} class="ml-auto shrink-0" />
					{/if}
				</div>
				<ul class="m-0 mt-2 flex list-none flex-col gap-1.5 p-0">
					{#each site.groups as group (group.category)}
						<li class="grid grid-cols-[5rem_minmax(0,1fr)_auto] items-center gap-2.5 text-[0.8125rem]">
							<span class="truncate text-ink-2">{categoryLabel(group.category)}</span>
							<Squares items={group.monitors} />
							<span class={`text-right text-xs tabular-nums ${group.counts.down ? 'font-semibold text-warning-ink' : group.counts.degraded ? 'font-semibold text-advisory-ink' : 'text-ink-2'}`}>
								{m.w9layout_count_up_of({ up: group.counts.up, total: group.counts.active })}
							</span>
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	</DataState>
</WidgetCard>
