<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The overview's fleet health. At the top, one small tile per monitor (its
	 * state as a shape and a colour, never colour alone; the name in the tooltip
	 * and for screen readers), problems first, shrinking as the fleet grows.
	 * What is wrong, with its detail and quick action, is "Needs attention"
	 * beside it: the same monitor is never listed twice.
	 */
	import type { Target } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { splitTiles, tileSize } from './health.js';
	import { STATUS_MARK, STATUS_RANK, statusWord, type ConsoleStatus } from './status.js';

	interface Row {
		target: Target;
		status: ConsoleStatus;
	}
	interface Props {
		rows: Row[];
	}
	let { rows }: Props = $props();

	/** Tile outline by state: the problems stand out before one reads a word. */
	const TILE_EDGE: Record<ConsoleStatus, string> = {
		down: 'border-warning',
		degraded: 'border-advisory',
		config: 'border-advisory',
		waiting: 'border-line',
		up: 'border-line',
		paused: 'border-line'
	};

	const sorted = $derived(
		[...rows].sort(
			(a, b) => STATUS_RANK[a.status] - STATUS_RANK[b.status] || a.target.name.localeCompare(b.target.name)
		)
	);
	const tiles = $derived(splitTiles(sorted));
	const size = $derived(tileSize(sorted.length));
</script>

<section class="min-w-0 rounded-xl border border-line bg-surface px-4 pt-3.5 pb-4" aria-labelledby="fleet-title">
	<h2 id="fleet-title" class="m-0 text-base font-semibold text-ink">{m.overview_fleet_title()}</h2>

	<ul class="m-0 mt-3 grid list-none gap-1 p-0" style={`grid-template-columns: repeat(auto-fill, minmax(${size}px, 1fr))`} aria-label={m.overview_fleet_tiles_label()}>
		{#each tiles.shown as row (row.target.id)}
			{@const label = m.overview_fleet_tile_title({ name: row.target.name, status: statusWord(row.status) })}
			<li class="min-w-0">
				<a
					href={withBase(`/targets/${row.target.id}`)}
					title={label}
					aria-label={label}
					class={`grid aspect-square w-full place-items-center rounded-md border bg-surface-2 transition-colors hover:bg-canvas ${TILE_EDGE[row.status]}`}
				>
					<span aria-hidden="true" class={`block size-[42%] ${STATUS_MARK[row.status]}`}></span>
				</a>
			</li>
		{/each}
	</ul>
	{#if tiles.hidden > 0}
		<a href={withBase('/targets')} class="mt-1 inline-flex min-h-10 items-center text-xs font-medium text-ink-2 hover:text-ink hover:underline">{m.overview_fleet_more({ count: tiles.hidden })}</a>
	{/if}
</section>
