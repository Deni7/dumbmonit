<script lang="ts">
	/**
	 * The overview: the whole picture on one screen, without scrolling the
	 * page. The three figures in a short row, then a grid of three columns
	 * and two rows that fills the rest of the window:
	 *
	 *   | Act now (2 columns)          | Coming up     |
	 *   | Services      | Fleet by site | Recent events |
	 *
	 * "Act now" gets the most room (two thirds of the width, the taller row).
	 * Panels in a row share their top and bottom edges. Without any service
	 * check, "Fleet by site" takes both columns of the bottom row.
	 *
	 * On a wide screen (lg and up) the view is exactly as tall as the window
	 * leaves it (`fitViewport`; a CSS estimate until it is measured) and every
	 * panel scrolls inside itself. From md, two columns ("Act now" across
	 * both); narrower, the panels stack and the page scrolls as usual.
	 */
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
	class="flex min-w-0 flex-col gap-3 lg:h-[var(--overview-h,calc(100dvh_-_10.5rem))]"
	style={height ? `--overview-h:${height}px` : undefined}
>
	<KpiTiles class="shrink-0" />
	<div
		class="grid min-w-0 grid-cols-1 gap-3 md:grid-cols-2 lg:min-h-0 lg:flex-1 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(18rem,1fr)] lg:grid-rows-[minmax(0,1.5fr)_minmax(0,1fr)]"
	>
		<ProblemsTable class="md:col-span-2 lg:col-start-1 lg:row-start-1" />
		<ComingUpCard class="lg:col-start-3 lg:row-start-1" />
		<RecentEvents class="lg:col-start-3 lg:row-start-2" />
		{#if services}<ServicesCompact class="lg:col-start-1 lg:row-start-2" />{/if}
		<SiteHealth class={services ? 'lg:col-start-2 lg:row-start-2' : 'md:col-span-2 lg:col-start-1 lg:row-start-2'} />
	</div>
</div>
