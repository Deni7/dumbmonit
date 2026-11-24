<script lang="ts" module>
	let instances = 0;
</script>

<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Recent events, newest first: alerts firing, resolving and acknowledged,
	 * and the changes around them (agent installs, edits, maintenance). One
	 * line each: when, a glyph in the event's tone, what happened (the
	 * server's sentence). A small filter keeps only alerts or only changes.
	 * The time column is as wide as its longest entry ("Wed 10:19 PM"), the
	 * same for every line, so a time never runs into its glyph. `fill`: the
	 * card takes its parent's height and scrolls inside. From the dashboards'
	 * store.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { ErrorNotice, Segmented, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import Glyph from '../parts/Glyph.svelte';
	import { EVENT_GLYPH, EVENT_TONE, TONE_SOFT, eventGroup } from '../parts/plates.js';
	import { clockText } from '../parts/words.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 8, limit = 8, fill = false }: { span?: Span; limit?: number; fill?: boolean } = $props();

	const name = `dash-events-${(instances += 1)}`;
	let filter = $state('all');
	const options = $derived([
		{ value: 'all', label: m.w9dash_events_all() },
		{ value: 'alerts', label: m.w9dash_events_alerts() },
		{ value: 'changes', label: m.w9dash_events_changes() }
	]);

	const events = $derived(widgetsStore.data?.events ?? null);
	const shown = $derived(
		(events ?? []).filter((e) => filter === 'all' || eventGroup(e.kind) === filter).slice(0, limit)
	);
</script>

<WidgetCard title={m.w9dash_events_title()} {span} {fill}>
	{#snippet right()}
		{#if events && events.length > 0}
			<Segmented legend={m.w9dash_events_filter()} hideLegend {options} bind:value={filter} {name} class={fill ? '-my-1.5' : ''} />
		{/if}
	{/snippet}
	{#if !events && widgetsStore.loading}
		<Skeleton class="h-5 w-full" rows={4} />
	{:else if !events}
		<ErrorNotice error={widgetsStore.error} title={m.w9dash_common_error()} compact onretry={() => void widgetsStore.refresh()} />
	{:else if shown.length === 0}
		<p class="m-0 py-3 text-sm text-ink-2">{events.length === 0 ? m.w9dash_events_empty() : m.w9dash_events_empty_filter()}</p>
	{:else}
		<ul class="m-0 grid list-none grid-cols-[max-content_1.125rem_minmax(0,1fr)] gap-x-2.5 p-0">
			{#each shown as event, i (`${event.at}-${i}`)}
				<li class={`col-span-3 grid grid-cols-subgrid items-start py-[7px] text-[0.8125rem] ${i > 0 ? 'border-t border-line' : ''}`}>
					<time class="pt-px text-xs whitespace-nowrap text-ink-3 tabular-nums" datetime={event.at}>{clockText(event.at)}</time>
					<span class={`grid size-[1.125rem] place-items-center rounded-[5px] ${TONE_SOFT[EVENT_TONE[event.kind]]}`}>
						<Glyph name={EVENT_GLYPH[event.kind]} class="size-[11px]" />
					</span>
					{#if event.target_id !== null}
						<a href={withBase(`/targets/${event.target_id}`)} class="min-w-0 text-ink hover:underline">{event.text}</a>
					{:else}
						<span class="min-w-0 text-ink">{event.text}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet foot()}
		<a href={withBase('/alerts?tab=history')}>{m.w9dash_events_history()}</a>
		{#if auth.isAdmin && !fill}<a href={withBase('/settings/team#audit')}>{m.w9dash_events_audit()}</a>{/if}
	{/snippet}
</WidgetCard>
