<script lang="ts">
	/**
	 * Fleet by site: three rings, site inside, then category, then one segment
	 * per monitor. Monitors down or degraded are named at the edge with their
	 * glyph (× or !), so the answer to "where?" reads without colour; every
	 * segment names itself on hover. The centre holds the one figure: up of watched.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import StatusLegend from '../parts/StatusLegend.svelte';
	import { SUN, clip, sunburstLayout } from '../parts/geometry.js';
	import { STATUS_FILL, STATUS_GLYPH, countStatuses, sortByCategory, type Span } from '../parts/sites.js';
	import { categoryLabel, monitorTitle } from '../parts/labels.js';

	let { span = 4 }: { span?: Span } = $props();

	const data = $derived(widgetsStore.data);
	const sites = $derived((data?.sites ?? []).filter((site) => site.monitors.length > 0).map((site) => ({ ...site, monitors: sortByCategory(site.monitors) })));
	const layout = $derived(sunburstLayout(sites));
	const counts = $derived(countStatuses(sites.flatMap((site) => site.monitors)));
	const names = $derived(new Map(sites.map((site) => [site.key, site.name])));

	/** Neutral shades for the site and category rings: status colours stay for monitors. */
	const SHADE = [0.62, 0.42, 0.26];
	const shade = (index: number) => SHADE[index % SHADE.length];

	const LABEL_INK: Record<string, string> = {
		down: 'var(--c-warning-ink)',
		degraded: 'var(--c-advisory-ink)'
	};
</script>

<WidgetCard {span} title={m.w9layout_sun_title()} sub={m.w9layout_sun_sub()}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={layout.total === 0} emptyText={m.w9layout_fleet_empty()}>
		<div class="flex flex-col items-center gap-2">
			<svg
				viewBox="-230 -150 460 300"
				class="block h-auto w-full max-w-[380px]"
				role="img"
				aria-label={m.w9layout_sun_aria({ up: counts.up, count: counts.active, down: counts.down, degraded: counts.degraded })}
			>
				<circle r={SUN.hole} fill="var(--c-surface-2)" />
				<text y="-2" text-anchor="middle" font-size="24" font-weight="650" fill="var(--c-ink)" class="font-mono tabular-nums">{counts.up}/{counts.active}</text>
				<text y="15" text-anchor="middle" font-size="10" fill="var(--c-ink-2)">{m.w9layout_sun_center()}</text>

				{#each layout.sites as arc (arc.key)}
					<path d={arc.d} fill="var(--c-ink-3)" fill-opacity={shade(arc.index)}><title>{arc.name}</title></path>
				{/each}
				{#each layout.categories as arc (`${arc.site}:${arc.category}`)}
					<path d={arc.d} fill="var(--c-ink-3)" fill-opacity={shade(arc.siteIndex) * 0.6}>
						<title>{m.w9layout_sun_category_title({ site: names.get(arc.site) ?? '', category: categoryLabel(arc.category), count: arc.count })}</title>
					</path>
				{/each}
				{#each layout.monitors as arc (arc.monitor.id)}
					<path d={arc.d} fill={STATUS_FILL[arc.monitor.status]}><title>{monitorTitle(arc.monitor)}</title></path>
				{/each}
				{#each layout.sites as arc (arc.key)}
					{#if arc.label}
						<text x={arc.label.x.toFixed(1)} y={arc.label.y.toFixed(1)} text-anchor="middle" font-size="9.5" font-weight="700" fill="var(--c-ink)" aria-hidden="true">{clip(arc.name, 9)}</text>
					{/if}
				{/each}
				{#each layout.labels as label (label.monitor.id)}
					<line x1={label.x1.toFixed(1)} y1={label.y1.toFixed(1)} x2={label.x2.toFixed(1)} y2={label.y2.toFixed(1)} stroke="var(--c-ink-3)" />
					<text x={label.tx.toFixed(1)} y={(label.y2 + 3).toFixed(1)} font-size="10" font-weight="650" text-anchor={label.anchor} fill={LABEL_INK[label.monitor.status] ?? 'var(--c-ink-2)'}>
						{STATUS_GLYPH[label.monitor.status]} {clip(label.monitor.name, 13)}
					</text>
				{/each}
			</svg>
			<p class="m-0 text-center text-xs text-ink-2">{m.w9layout_sun_rings()}</p>
			<StatusLegend class="justify-center" />
		</div>
	</DataState>
</WidgetCard>
