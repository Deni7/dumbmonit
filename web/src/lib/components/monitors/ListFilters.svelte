<script lang="ts">
	/**
	 * What "Filter" unfolds over the monitors list: the state segment, the kind,
	 * the site, the free tags and the sort, with "Clear filters" once one is set.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/ui/index.js';
	import Segmented from '#lib/components/devices/Segmented.svelte';
	import type { Segment, SortDir, SortKey } from '#lib/components/devices/listState.js';
	import type { MapLocation } from '#lib/api/types.js';
	import { ArrowUpDown } from 'lucide-svelte';
	import TagFilter from './TagFilter.svelte';

	interface Props {
		segment: Segment;
		kind: string;
		site: string;
		tags: string[];
		sort: SortKey;
		sortDir: SortDir;
		segments: { id: Segment; label: string; count: number }[];
		/** `[kind, label]` pairs, already sorted. */
		kinds: [string, string][];
		siteLocations: MapLocation[];
		showSiteFilter: boolean;
		/** Free tags in use with their device count. */
		tagChips: [string, number][];
		sortOptions: { id: SortKey; label: string }[];
		sorted: boolean;
		activeFilters: number;
		onclear: () => void;
	}

	let {
		segment = $bindable(),
		kind = $bindable(),
		site = $bindable(),
		tags = $bindable(),
		sort = $bindable(),
		sortDir = $bindable(),
		segments,
		kinds,
		siteLocations,
		showSiteFilter,
		tagChips,
		sortOptions,
		sorted,
		activeFilters,
		onclear
	}: Props = $props();
</script>

<section
	id="monitors-filters"
	aria-label={m.devices_targets_filter_panel()}
	class="mb-4 flex flex-col gap-4 rounded-[var(--radius-card)] border border-line bg-surface p-4"
>
	<div class="flex flex-col gap-1.5">
		<span class="label-tape">{m.devices_list_sort_status()}</span>
		<Segmented options={segments} value={segment} onchange={(v) => (segment = v)} label={m.devices_targets_filter_state()} class="self-start max-sm:w-full max-sm:flex-wrap" />
	</div>
	<div class="flex flex-wrap gap-4">
		<label class="flex min-w-0 flex-col gap-1.5 max-sm:w-full">
			<span class="label-tape">{m.devices_list_sort_kind()}</span>
			<select class="input !min-h-10 !w-auto max-w-full !py-1.5 text-sm max-sm:!w-full" bind:value={kind} aria-label={m.devices_targets_filter_kind()}>
				<option value="">{m.devices_targets_all_kinds()}</option>
				{#each kinds as [value, label] (value)}
					<option {value}>{label}</option>
				{/each}
			</select>
		</label>
		{#if showSiteFilter}
			<label class="flex min-w-0 flex-col gap-1.5 max-sm:w-full">
				<span class="label-tape">{m.devices_targets_filter_site_label()}</span>
				<select class="input !min-h-10 !w-auto max-w-full !py-1.5 text-sm max-sm:!w-full" bind:value={site} aria-label={m.devices_targets_filter_site()}>
					<option value="">{m.devices_targets_all_sites()}</option>
					{#each siteLocations as location (location.id)}
						<option value={String(location.id)}>{location.name}</option>
					{/each}
					<option value="none">{m.devices_targets_no_site()}</option>
				</select>
			</label>
		{/if}
		{#if tagChips.length > 0 || tags.length > 0}
			<div class="flex min-w-0 flex-col gap-1.5 max-sm:w-full">
				<span class="label-tape">{m.devices_targets_filter_tags_label()}</span>
				<TagFilter chips={tagChips} value={tags} onchange={(next) => (tags = next)} />
			</div>
		{/if}
		<div class="flex min-w-0 flex-col gap-1.5">
			<label class="label-tape" for="monitors-sort">{m.devices_list_sort_label()}</label>
			<div class="flex items-center gap-1">
				<select id="monitors-sort" class="input !min-h-10 !w-auto max-w-full !py-1.5 text-sm" bind:value={sort}>
					{#each sortOptions as option (option.id)}
						<option value={option.id}>{option.label}</option>
					{/each}
				</select>
				{#if sorted}
					<button
						type="button"
						class="inline-flex size-10 shrink-0 items-center justify-center rounded-lg border border-line text-ink-2 hover:bg-surface-2 hover:text-ink"
						aria-pressed={sortDir === 'desc'}
						aria-label={m.devices_list_sort_reverse()}
						title={m.devices_list_sort_reverse()}
						onclick={() => (sortDir = sortDir === 'asc' ? 'desc' : 'asc')}
					>
						<ArrowUpDown class="size-4" aria-hidden="true" />
					</button>
				{/if}
			</div>
		</div>
	</div>
	{#if activeFilters > 0}
		<div>
			<Button variant="ghost" class="-ml-3" onclick={onclear}>{m.devices_targets_clear_filters()}</Button>
		</div>
	{/if}
</section>
