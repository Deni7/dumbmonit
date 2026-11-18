<script lang="ts">
	/**
	 * "Legend" button of the map's toolbar and the panel it opens: what the
	 * circles, badges, frames and lines of the map mean. Only the kinds of
	 * link the map actually draws are listed. Escape or a click elsewhere
	 * closes it. The exported sheet draws its own copy.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { STATE_LABEL } from '#lib/format.js';
	import type { TopologySource } from '#lib/api/index.js';
	import { Button } from '#lib/ui/index.js';
	import { Info, X } from 'lucide-svelte';
	import { EDGE_DASH, SOURCES, sourceLabel } from './legend';
	import { BAND_LIMITS, widthFor } from './traffic';

	interface Props {
		/** Kinds of link on the map; every kind when not given. */
		sources?: ReadonlySet<TopologySource>;
		/** Sites are drawn: the site frame and the WAN link are part of the legend. */
		wan?: boolean;
		/** Some links were corrected by hand or set by a learned rule. */
		corrected?: boolean;
		/** The weathermap is on: add the load scale. */
		traffic?: boolean;
		class?: string;
	}

	let { sources, wan = false, corrected = false, traffic = false, class: className = '' }: Props = $props();

	let open = $state(false);
	let root = $state<HTMLDivElement | null>(null);
	const panelId = 'topo-legend-panel';

	/** A link attached by hand is a dependency too: listed only when the map has one. */
	const shown = $derived<TopologySource[]>(sources ? [...SOURCES, 'manual' as const].filter((s) => sources?.has(s)) : [...SOURCES]);

	/** The four load bands of the weathermap: 0–25, 25–50, 50–75, 75–100 %. */
	const BANDS = [
		{ from: 0, to: BAND_LIMITS[0], stroke: 'stroke-signal' },
		{ from: BAND_LIMITS[0], to: BAND_LIMITS[1], stroke: 'stroke-signal' },
		{ from: BAND_LIMITS[1], to: BAND_LIMITS[2], stroke: 'stroke-advisory' },
		{ from: BAND_LIMITS[2], to: 100, stroke: 'stroke-warning' }
	];

	type Badge = 'ok' | 'warn' | 'wait' | 'idle' | 'down' | 'impacted';
	const BADGES: { badge: Badge; label: () => string }[] = [
		{ badge: 'ok', label: () => STATE_LABEL.online },
		{ badge: 'warn', label: () => STATE_LABEL.misconfigured },
		{ badge: 'wait', label: () => STATE_LABEL.pending },
		{ badge: 'idle', label: () => m.topology_legend_paused() },
		{ badge: 'down', label: () => m.topology_legend_down() },
		{ badge: 'impacted', label: () => m.topology_legend_suppressed() }
	];

	$effect(() => {
		if (!open) return;
		const onpointerdown = (event: PointerEvent) => {
			if (root && !root.contains(event.target as Node)) open = false;
		};
		const onkeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') {
				event.stopPropagation();
				open = false;
			}
		};
		document.addEventListener('pointerdown', onpointerdown);
		document.addEventListener('keydown', onkeydown);
		return () => {
			document.removeEventListener('pointerdown', onpointerdown);
			document.removeEventListener('keydown', onkeydown);
		};
	});

	const row = 'flex min-h-7 items-center gap-3';
	const heading = 'mb-1.5 text-xs font-semibold text-ink';
</script>

{#snippet line(dash: string, arrow: boolean)}
	<svg width="40" height="12" aria-hidden="true" class="shrink-0">
		{#if arrow}
			<line x1="2" y1="6" x2="30" y2="6" class="stroke-ink-2" stroke-width="1.6" stroke-linecap="round" />
			<polygon points="38,6 30,2 30,10" class="fill-ink-2" />
		{:else}
			<line x1="2" y1="6" x2="38" y2="6" class="stroke-ink-3" stroke-width="1.4" stroke-linecap="round" stroke-dasharray={dash || undefined} />
		{/if}
	</svg>
{/snippet}

{#snippet badge(kind: Badge)}
	<svg width="40" height="24" viewBox="-20 -12 40 24" aria-hidden="true" class="shrink-0">
		{#if kind === 'down'}
			<circle r="9" class="fill-warning-soft stroke-warning" stroke-width="2.5" />
		{:else if kind === 'impacted'}
			<circle r="9" class="fill-surface stroke-advisory" stroke-width="1.8" stroke-dasharray="3 2.5" />
		{:else if kind === 'idle'}
			<circle r="9" class="fill-surface-2 stroke-ink-3" stroke-width="1.5" stroke-dasharray="2 3" />
		{:else}
			<circle r="9" class={`fill-surface ${kind === 'ok' ? 'stroke-signal' : kind === 'warn' ? 'stroke-advisory' : 'stroke-ink-3'}`} stroke-width="2" />
		{/if}
		<g transform="translate(7,-5)">
			<circle r="6" class={`${kind === 'ok' ? 'fill-signal' : kind === 'down' ? 'fill-warning' : kind === 'warn' || kind === 'impacted' ? 'fill-advisory' : 'fill-ink-3'} stroke-surface`} stroke-width="1.5" />
			<g class="stroke-surface" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none">
				{#if kind === 'ok'}
					<path d="M-2.4,0.2 L-0.8,1.8 L2.4,-1.6" />
				{:else if kind === 'down' || kind === 'warn'}
					<path d="M0,-2.7 L0,0.5 M0,2.6 L0,2.7" />
				{:else if kind === 'impacted'}
					<path d="M0,-2.7 L0,2.4 M-2.1,0.5 L0,2.6 L2.1,0.5" />
				{:else if kind === 'wait'}
					<path d="M-2.4,0 L-2.3,0 M0,0 L0.1,0 M2.4,0 L2.5,0" />
				{:else}
					<path d="M-1.3,-2.1 L-1.3,2.1 M1.3,-2.1 L1.3,2.1" />
				{/if}
			</g>
		</g>
	</svg>
{/snippet}

<div class={`relative ${className}`} bind:this={root}>
	<Button variant="secondary" aria-expanded={open} aria-controls={panelId} onclick={() => (open = !open)}>
		<Info class="size-4" aria-hidden="true" />
		{m.topology_legend_title()}
	</Button>
	{#if open}
		<div
			id={panelId}
			role="dialog"
			aria-label={m.topology_legend_title()}
			class="motion-pop absolute top-full left-0 z-30 mt-2 max-h-[min(70vh,36rem)] w-[min(24rem,calc(100vw-2rem))] overflow-y-auto rounded-[var(--radius-card)] border border-line bg-surface p-4 text-sm text-ink-2 shadow-float sm:right-0 sm:left-auto"
		>
			<div class="mb-3 flex items-center justify-between gap-3">
				<h2 class="text-sm font-bold text-ink">{m.topology_legend_title()}</h2>
				<button
					type="button"
					class="-my-2 -mr-2 inline-flex size-10 items-center justify-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
					aria-label={m.topology_legend_close()}
					onclick={() => (open = false)}
				>
					<X class="size-4" aria-hidden="true" />
				</button>
			</div>

			<section class="mb-4">
				<h3 class={heading}>{m.topology_legend_nodes()}</h3>
				<ul class="flex flex-col gap-1">
					<li class={row}>
						<svg width="40" height="24" viewBox="-20 -12 40 24" aria-hidden="true" class="shrink-0">
							<circle r="10.5" class="fill-surface stroke-ink-2" stroke-width="1.8" />
						</svg>
						{m.topology_legend_hub()}
					</li>
					<li class={row}>
						<svg width="40" height="24" viewBox="-20 -12 40 24" aria-hidden="true" class="shrink-0">
							<circle r="7.5" class="fill-surface stroke-ink-2" stroke-width="1.8" />
						</svg>
						{m.topology_legend_leaf()}
					</li>
					{#each BADGES as entry (entry.badge)}
						<li class={row}>{@render badge(entry.badge)}{entry.label()}</li>
					{/each}
				</ul>
			</section>

			<section class={wan || traffic ? 'mb-4' : ''}>
				<h3 class={heading}>{m.topology_legend_links()}</h3>
				<ul class="flex flex-col gap-1">
					{#each shown as source (source)}
						<li class={row}>{@render line(EDGE_DASH[source], source === 'parent' || source === 'manual')}{sourceLabel(source)}</li>
					{/each}
					{#if corrected}
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="2" y1="6" x2="38" y2="6" class="stroke-ink-2" stroke-width="1.6" stroke-linecap="round" />
								<path d="M20,0.5 L25.5,6 L20,11.5 L14.5,6 Z" class="fill-surface stroke-ink-2" stroke-width="1.6" />
							</svg>
							{m.topology_legend_corrected()}
						</li>
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="2" y1="6" x2="38" y2="6" class="stroke-ink-2" stroke-width="1.6" stroke-linecap="round" />
								<path d="M20,0.5 L25.5,6 L20,11.5 L14.5,6 Z" class="fill-ink-2 stroke-ink-2" stroke-width="1.6" />
							</svg>
							{m.topology_legend_learned()}
						</li>
					{/if}
				</ul>
			</section>

			{#if wan}
				<section class={traffic ? 'mb-4' : ''}>
					<h3 class={heading}>{m.topology_legend_sites()}</h3>
					<ul class="flex flex-col gap-1">
						<li class={row}>
							<svg width="40" height="24" aria-hidden="true" class="shrink-0">
								<rect x="3" y="3" width="34" height="18" rx="6" class="fill-surface-2 stroke-line-strong" stroke-width="1.5" stroke-dasharray="5 3" />
							</svg>
							{m.topology_legend_site()}
						</li>
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="4" y1="6" x2="36" y2="6" class="stroke-ink-3" stroke-width="7" stroke-linecap="round" opacity="0.14" />
								<line x1="4" y1="6" x2="36" y2="6" class="stroke-ink-3" stroke-width="2.2" stroke-linecap="round" stroke-dasharray={EDGE_DASH.wan} />
							</svg>
							{sourceLabel('wan')}
						</li>
					</ul>
				</section>
			{/if}

			{#if traffic}
				<section>
					<h3 class={heading}>{m.topology_traffic_legend_title()}</h3>
					<ul class="flex flex-col gap-1">
						{#each BANDS as band (band.from)}
							<li class={row}>
								<svg width="40" height="12" aria-hidden="true" class="shrink-0">
									<line x1="3" y1="6" x2="37" y2="6" class={band.stroke} stroke-width={widthFor((band.from + band.to) / 2)} stroke-linecap="round" />
								</svg>
								<span class="tabular-nums">{m.topology_traffic_legend_range({ from: band.from, to: band.to })}</span>
							</li>
						{/each}
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="2" y1="6" x2="38" y2="6" class="stroke-advisory" stroke-width="3" stroke-dasharray="5 3" />
							</svg>
							{m.topology_traffic_legend_errors()}
						</li>
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="3" y1="6" x2="37" y2="6" class="stroke-signal" stroke-width="4" stroke-linecap="round" />
								<line x1="3" y1="6" x2="37" y2="6" class="stroke-surface" stroke-width="2" stroke-linecap="round" stroke-dasharray="0.1 10" />
							</svg>
							{m.topology_traffic_legend_flow()}
						</li>
						<li class={row}>
							<svg width="40" height="12" aria-hidden="true" class="shrink-0">
								<line x1="2" y1="6" x2="38" y2="6" class="stroke-ink-3" stroke-width="3" stroke-dasharray="2 3" stroke-linecap="round" />
							</svg>
							{m.topology_traffic_legend_stale()}
						</li>
					</ul>
				</section>
			{/if}
		</div>
	{/if}
</div>
