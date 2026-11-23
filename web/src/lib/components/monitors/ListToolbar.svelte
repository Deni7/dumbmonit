<script lang="ts">
	/**
	 * The toolbar of the monitors list: search and Filter on the left; the
	 * Compact/Tiles switch, a quiet "Create folder" (a small popover with the
	 * name field) and "Select" on the right (both for administrators).
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, Menu } from '#lib/ui/index.js';
	import ViewSwitcher from '#lib/components/devices/ViewSwitcher.svelte';
	import type { RackView } from '#lib/components/devices/view.js';
	import { FolderPlus, ListChecks, Search, SlidersHorizontal } from 'lucide-svelte';

	interface Props {
		search: string;
		filtersOpen: boolean;
		/** Filters away from their default: the count on the Filter button. */
		activeFilters: number;
		view: RackView;
		onviewchange: (view: RackView) => void;
		selecting: boolean;
		ontoggleselect: () => void;
		oncreatefolder: (name: string) => void;
	}

	let {
		search = $bindable(),
		filtersOpen = $bindable(),
		activeFilters,
		view,
		onviewchange,
		selecting,
		ontoggleselect,
		oncreatefolder
	}: Props = $props();

	// "Create folder": an in-app name field in a popover, never `window.prompt`.
	let newFolderDraft = $state('');
</script>

<!--
	The toolbar of the Alerts list, at its size: 32 px controls from `sm`
	(40 px on phones), quiet ghost buttons, one switch. Phones: the search
	on its own line, the controls under it, and the bar stays put while the
	list scrolls under it.
-->
<div class="mb-4 flex flex-wrap items-center gap-x-2 gap-y-2 max-sm:sticky max-sm:top-[calc(var(--safe-top)_+_var(--tabs-h,0px))] max-sm:z-10 max-sm:-mx-4 max-sm:mb-2 max-sm:border-b max-sm:border-line max-sm:bg-canvas max-sm:px-4 max-sm:py-2">
	<label class="relative min-w-0 basis-full sm:max-w-xs sm:flex-1 sm:basis-auto">
		<span class="sr-only">{m.devices_targets_search_label()}</span>
		<Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
		<input
			type="search"
			data-page-search
			class="input !pl-8 sm:!min-h-8 sm:!py-1"
			placeholder={m.devices_targets_search_placeholder()}
			bind:value={search}
			autocomplete="off"
		/>
	</label>
	<Button
		size="sm"
		variant="ghost"
		data-filter-toggle
		aria-expanded={filtersOpen}
		aria-controls="monitors-filters"
		aria-label={activeFilters > 0 ? m.devices_targets_filter_aria({ count: activeFilters }) : undefined}
		class={filtersOpen ? 'bg-surface-2 text-ink' : ''}
		onclick={() => (filtersOpen = !filtersOpen)}
	>
		<SlidersHorizontal class="size-4" aria-hidden="true" />
		{m.devices_targets_filter()}
		{#if activeFilters > 0}
			<span class="tnum -mr-1 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-ink px-1 text-[0.6875rem] leading-none text-surface">{activeFilters}</span>
		{/if}
	</Button>
	<span class="flex-1" aria-hidden="true"></span>
	<div class="flex items-center gap-2">
		<ViewSwitcher value={view} onchange={onviewchange} />
		{#if auth.can('monitors.manage')}
			<Menu label={m.devices_targets_create_folder()} align="right">
				{#snippet trigger({ toggle, open })}
					<Button
						size="sm"
						variant="ghost"
						aria-haspopup="menu"
						aria-expanded={open}
						aria-label={m.devices_targets_create_folder()}
						title={m.devices_targets_create_folder()}
						class={`!w-10 !px-0 sm:!w-8 ${open ? 'bg-surface-2 text-ink' : ''}`}
						onclick={() => {
							newFolderDraft = '';
							toggle();
						}}
					>
						<FolderPlus class="size-4" aria-hidden="true" />
					</Button>
				{/snippet}
				{#snippet children({ close })}
					<form
						class="flex w-72 max-w-full items-center gap-1 p-0.5"
						onsubmit={(e) => {
							e.preventDefault();
							if (!newFolderDraft.trim()) return;
							oncreatefolder(newFolderDraft);
							newFolderDraft = '';
							close();
						}}
					>
						<input
							bind:value={newFolderDraft}
							class="input !h-10 min-w-0 flex-1 text-[0.8125rem]"
							placeholder={m.devices_targets_folder_placeholder()}
							maxlength="80"
							aria-label={m.devices_targets_folder_aria()}
							autocomplete="off"
							{@attach (node) => {
								const timer = setTimeout(() => node.focus(), 0);
								return () => clearTimeout(timer);
							}}
						/>
						<Button type="submit" size="sm" variant="secondary" disabled={!newFolderDraft.trim()}>{m.devices_targets_folder_create()}</Button>
					</form>
				{/snippet}
			</Menu>
			<Button
				size="sm"
				variant="ghost"
				aria-pressed={selecting}
				aria-label={m.devices_list_select()}
				title={m.devices_list_select()}
				class={`max-sm:!w-10 max-sm:!px-0 ${selecting ? 'bg-surface-2 text-ink' : ''}`}
				onclick={ontoggleselect}
			>
				<ListChecks class="size-4" aria-hidden="true" />
				<span class="max-sm:sr-only">{m.devices_list_select()}</span>
			</Button>
		{/if}
	</div>
</div>
