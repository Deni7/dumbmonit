<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The overview's fleet health. At the top, one small tile per monitor (its
	 * state as a shape and a colour, never colour alone; the name in the tooltip
	 * and for screen readers), problems first, shrinking as the fleet grows.
	 * Below, in the foreground, only what is wrong: each monitor with its last
	 * 24 hours as a bar, how long it has been down and its last error. When
	 * nothing is wrong, one calm line.
	 */
	import type { Target, TargetId } from '#lib/api/index.js';
	import { formatDuration, formatFailureReason } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { type RiverCell, RIVER_SLOTS } from './fleet.js';
	import { dayAvailability, isProblem, outageSeconds, splitTiles, tileSize } from './health.js';
	import { STATUS_MARK, STATUS_RANK, STATUS_TEXT, statusWord, type ConsoleStatus } from './status.js';

	interface Row {
		target: Target;
		status: ConsoleStatus;
		/** Why the last probe failed, when it did. */
		reason: string | null;
	}
	interface Props {
		rows: Row[];
		/** The last 24 hours of each monitor, from the fleet store. */
		day: Map<TargetId, RiverCell[]>;
		/** Seconds the `day` covers. */
		daySeconds: number;
	}
	let { rows, day, daySeconds }: Props = $props();

	/** Problem rows shown; the rest is one click away in the Monitors list. */
	const PROBLEM_LIMIT = 8;
	const BAR: Record<RiverCell, string> = {
		up: 'bg-river-up',
		warn: 'bg-river-warn',
		down: 'bg-river-down',
		none: 'bg-river-none'
	};
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
	const problems = $derived(sorted.filter((row) => isProblem(row.status)));
	const shownProblems = $derived(problems.slice(0, PROBLEM_LIMIT));
	const slotSeconds = $derived(daySeconds / RIVER_SLOTS);

	const cellsOf = (id: TargetId): RiverCell[] => day.get(id) ?? [];
	function detail(row: Row): string {
		if (row.target.last_error) return row.target.last_error;
		return row.status === 'down' ? formatFailureReason(row.reason) : '';
	}
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

	{#if problems.length === 0}
		<p class="m-0 mt-4 flex min-h-10 items-center gap-2 border-t border-line pt-3 text-sm font-medium text-signal-ink">
			<span aria-hidden="true" class={`size-2.5 shrink-0 ${STATUS_MARK.up}`}></span>
			{m.overview_fleet_calm({ count: sorted.length })}
		</p>
	{:else}
		<h3 class="m-0 mt-4 border-t border-line pt-3 text-sm font-semibold text-ink">{m.overview_fleet_wrong_title()}</h3>
		<ul class="m-0 mt-1 list-none p-0">
			{#each shownProblems as row (row.target.id)}
				{@const cells = cellsOf(row.target.id)}
				{@const down = outageSeconds(cells, slotSeconds)}
				{@const availability = dayAvailability(cells)}
				{@const why = detail(row)}
				<li class="border-t border-line first:border-t-0">
					<a href={withBase(`/targets/${row.target.id}`)} class="block min-h-10 rounded-md py-2.5 hover:bg-surface-2">
						<span class="flex items-center gap-2">
							<span aria-hidden="true" class={`size-2.5 shrink-0 ${STATUS_MARK[row.status]}`}></span>
							<span class="min-w-0 flex-1 truncate text-sm font-semibold text-ink">{row.target.name}</span>
							<span class={`shrink-0 text-xs font-medium ${STATUS_TEXT[row.status]}`}>
								{down > 0 ? m.overview_fleet_down_for({ duration: formatDuration(down) }) : statusWord(row.status)}
							</span>
						</span>
						{#if why}
							<span class="mt-0.5 block truncate pl-[1.125rem] text-xs text-ink-2" title={why}>{why}</span>
						{/if}
						<span class="mt-1.5 flex items-center gap-2.5 pl-[1.125rem]">
							<span
								role="img"
								aria-label={m.overview_fleet_bar_label()}
								title={m.overview_fleet_bar_label()}
								class="grid h-2.5 min-w-0 flex-1 gap-px"
								style={`grid-template-columns: repeat(${RIVER_SLOTS}, minmax(0, 1fr))`}
							>
								{#each { length: RIVER_SLOTS } as _, i (i)}
									<span class={`rounded-[1px] ${BAR[cells[i] ?? 'none']}`}></span>
								{/each}
							</span>
							{#if availability !== null}
								<span class="shrink-0 font-mono text-xs text-ink-2 tabular-nums">{m.overview_fleet_availability({ percent: availability.toFixed(1) })}</span>
							{/if}
						</span>
					</a>
				</li>
			{/each}
		</ul>
		{#if problems.length > shownProblems.length}
			<a href={withBase('/targets')} class="mt-1 inline-flex min-h-10 items-center text-xs font-medium text-ink-2 hover:text-ink hover:underline">{m.overview_fleet_more({ count: problems.length - shownProblems.length })}</a>
		{/if}
	{/if}
</section>
