<script lang="ts">
	/**
	 * Dashboard C, "Hybrid": the whole picture on one screen, without scrolling
	 * the page. The three figures, then what to act on now (the largest panel,
	 * left), what is coming up (maintenance windows, expiries and forecasts, the
	 * update, in one list) and the fleet by site with a line per category on the
	 * right, and under the problems the service checks and the recent events.
	 *
	 * On a wide screen (lg and up) the view is exactly as tall as the window
	 * leaves it (`fitViewport`; a CSS estimate until it is measured) and every
	 * panel scrolls inside itself. Narrower, the panels stack and the page
	 * scrolls as usual. The service checks only take room when there are some.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import KpiTiles from './widgets/KpiTiles.svelte';
	import ProblemsTable from './widgets/ProblemsTable.svelte';
	import ComingUpCard from './widgets/ComingUpCard.svelte';
	import SiteHealth from './widgets/SiteHealth.svelte';
	import ServicesCompact from './widgets/ServicesCompact.svelte';
	import RecentEvents from './widgets/RecentEvents.svelte';
	import { widgetsStore } from './widgets.svelte.js';
	import { fitViewport } from './parts/fit-viewport.js';

	let root = $state<HTMLElement>();
	let height = $state<number | null>(null);
	$effect(() => {
		if (!root) return;
		return fitViewport(root, (px) => (height = px));
	});

	const services = $derived(widgetsStore.data ? widgetsStore.data.services.length > 0 : true);
</script>

<div
	bind:this={root}
	class="flex min-w-0 flex-col gap-3 lg:h-[var(--hybrid-h,calc(100dvh_-_11.75rem))]"
	style={height ? `--hybrid-h:${height}px` : undefined}
>
	<KpiTiles class="shrink-0" />
	<div class="grid min-w-0 gap-3 lg:min-h-0 lg:flex-1 lg:grid-cols-[minmax(0,2fr)_minmax(20rem,1fr)] lg:grid-rows-[minmax(0,1.6fr)_minmax(0,1fr)]">
		<ProblemsTable fill title={m.w9layout_hybrid_act_now()} />
		<div class="flex min-h-0 min-w-0 flex-col gap-3 lg:col-start-2 lg:row-span-2 lg:row-start-1">
			<ComingUpCard fill class="lg:max-h-[60%] lg:flex-[0_1_auto]" />
			<SiteHealth fill class="lg:flex-1" />
		</div>
		<div class={`grid min-h-0 min-w-0 gap-3 lg:grid-rows-[minmax(0,1fr)] ${services ? 'md:grid-cols-2' : ''}`}>
			{#if services}<ServicesCompact fill />{/if}
			<RecentEvents fill limit={20} />
		</div>
	</div>
</div>
