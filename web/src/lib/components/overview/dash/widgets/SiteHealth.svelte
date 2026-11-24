<script lang="ts">
	/**
	 * Fleet by site: per site, how many monitors answer, its health in a word,
	 * the WAN latency of a relay site, then one line per category with a
	 * status mark per monitor (the shared `StatusMark`: its glyph carries the
	 * state as well as its colour, the monitor's name in its tooltip) and the
	 * count up. Sites and categories come from the widgets store; each
	 * monitor's state is the app-wide one when known, so the figures agree
	 * with "Monitors up" and "Act now" (the server's view can be a poll
	 * behind). With a single site the count and the health word are left to
	 * "Monitors up". The panel scrolls, the title leads to the monitors.
	 */
	import type { WidgetStatus } from '#lib/api/index.js';
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Plate, StatusMark } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { uniqueKeys } from '../parts/keys.js';
	import { ROW, STATUS_MARK, TEXT_MAIN, TEXT_SIDE } from '../parts/scale.js';
	import { countStatuses, groupByCategory, healthOf, lastValue } from '../parts/sites.js';
	import { HEALTH_TONE, categoryLabel, healthWord, monitorTitle, num, statusWord } from '../parts/labels.js';

	let { class: cls = '' }: { class?: string } = $props();

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
	// Two relays can report the same site (its key from its name): keys stay unique.
	const siteKeys = $derived(uniqueKeys(sites, (site) => site.key));
	// One site: "Monitors up" above already gives its count and health; only the categories are new.
	const several = $derived(sites.length > 1);
</script>

<WidgetCard title={m.w9layout_sun_title()} href="/targets" class={cls}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={sites.length === 0} emptyText={m.w9layout_fleet_empty()} rows={4}>
		{#each sites as site, s (siteKeys[s])}
			<div class={ROW}>
				<div class="flex min-h-8 min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
					<h3 class={`m-0 min-w-0 truncate ${TEXT_MAIN}`}>{site.name}</h3>
					{#if several}
						<span class={`text-ink-2 ${TEXT_SIDE}`}>{site.counts.up} {m.w9layout_sites_up_of({ count: site.counts.active })}</span>
					{/if}
					{#if site.wan !== null}
						<span class={`text-ink-2 ${TEXT_SIDE}`}>{m.w9layout_sites_wan()} {m.w9layout_ms({ value: num(site.wan) })}</span>
					{/if}
					{#if several}
						<Plate tone={HEALTH_TONE[site.health]} label={healthWord(site.health, site.counts.down)} class="ml-auto shrink-0" />
					{/if}
				</div>
				<ul class="m-0 mt-1 flex list-none flex-col gap-1 p-0">
					<!-- Categories come once each (grouped): static order, keyed by position. -->
					{#each site.groups as group, g (g)}
						<li class="grid min-h-6 grid-cols-[5.5rem_minmax(0,1fr)_auto] items-center gap-3 text-[0.8125rem]">
							<span class="truncate text-ink-2">{categoryLabel(group.category)}</span>
							<span class="flex flex-wrap gap-1" aria-hidden="true">
								{#each group.monitors as monitor, k (k)}
									{@const mark = STATUS_MARK[monitor.status]}
									<StatusMark glyph={mark.glyph} tone={mark.tone} label={statusWord(monitor.status)} title={monitorTitle(monitor)} />
								{/each}
							</span>
							<span class={`text-right ${TEXT_SIDE} ${group.counts.down ? 'font-semibold text-warning-ink' : group.counts.degraded ? 'font-semibold text-advisory-ink' : 'text-ink-2'}`}>
								{m.w9layout_count_up_of({ up: group.counts.up, total: group.counts.active })}
							</span>
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	</DataState>
</WidgetCard>
