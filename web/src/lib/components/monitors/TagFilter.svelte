<script lang="ts">
	/**
	 * The tag filter of the monitor list as one compact control: a button that
	 * says what is chosen on one line ("prod, db +1", or "All tags") and opens
	 * a searchable checklist, with a × beside it to clear. It takes the place
	 * of a row of chips, whose width grew with every tag in use.
	 */
	import { Check, ChevronDown, Search, X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { viewport } from '#lib/stores/viewport.svelte.js';
	import { Menu } from '#lib/ui/index.js';
	import { tagChoices } from '#lib/components/devices/listState.js';

	interface Props {
		/** Free tags in use with their device count. */
		chips: [string, number][];
		value: string[];
		onchange: (next: string[]) => void;
	}

	let { chips, value, onchange }: Props = $props();

	let query = $state('');
	const choices = $derived(tagChoices(chips, value, query));

	/** The chosen tags on one line: the first two, then how many more. */
	const summary = $derived(
		value.length === 0
			? m.devices_targets_all_tags()
			: value.length <= 2
				? value.join(', ')
				: `${value.slice(0, 2).join(', ')} ${m.devices_targets_tags_more({ count: value.length - 2 })}`
	);

	function toggle(tag: string) {
		onchange(value.includes(tag) ? value.filter((t) => t !== tag) : [...value, tag]);
	}
</script>

<div class="flex min-w-0 items-center gap-1 max-sm:w-full">
	<Menu label={m.devices_targets_filter_tags()} align="left" class="min-w-0 max-sm:flex-1">
		{#snippet trigger({ toggle: open, open: isOpen })}
			<button
				type="button"
				class="input !flex !min-h-10 !w-auto max-w-full min-w-40 items-center justify-between gap-2 !py-1.5 text-left text-sm max-sm:!w-full"
				aria-haspopup="menu"
				aria-expanded={isOpen}
				aria-label={`${m.devices_targets_filter_tags()}: ${summary}`}
				onclick={() => {
					query = '';
					open();
				}}
			>
				<span class={`truncate ${value.length === 0 ? 'text-ink-2' : 'text-ink'}`}>{summary}</span>
				<ChevronDown class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
			</button>
		{/snippet}
		{#snippet children()}
			<div class="flex w-64 max-w-full flex-col gap-1">
				<label class="relative block">
					<span class="sr-only">{m.devices_targets_tags_search()}</span>
					<Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
					<input
						type="search"
						class="input !h-10 !pl-8 text-[0.8125rem]"
						placeholder={m.devices_targets_tags_search()}
						bind:value={query}
						autocomplete="off"
						{@attach (node) => {
							if (!viewport.phone) node.focus();
						}}
					/>
				</label>
				{#if choices.length === 0}
					<p class="px-2 py-2 text-[0.8125rem] text-ink-3">{m.devices_targets_tags_none()}</p>
				{:else}
					<ul class="flex max-h-64 flex-col overflow-y-auto" role="group" aria-label={m.devices_targets_filter_tags()}>
						{#each choices as [tag, count] (tag)}
							{@const on = value.includes(tag)}
							<li>
								<label class="flex min-h-10 cursor-pointer items-center gap-2 rounded-lg px-2 text-[0.8125rem] text-ink hover:bg-surface">
									<input type="checkbox" class="size-4 shrink-0 accent-[var(--c-signal)]" checked={on} onchange={() => toggle(tag)} />
									<span class="min-w-0 flex-1 break-all">{tag}</span>
									<span class="tnum text-ink-3">{count}</span>
									{#if on}<Check class="size-3.5 shrink-0 text-ink-2" aria-hidden="true" />{/if}
								</label>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/snippet}
	</Menu>
	{#if value.length > 0}
		<button
			type="button"
			class="inline-flex size-10 shrink-0 items-center justify-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
			aria-label={m.devices_targets_tags_clear()}
			title={m.devices_targets_tags_clear()}
			onclick={() => onchange([])}
		>
			<X class="size-4" aria-hidden="true" />
		</button>
	{/if}
</div>
