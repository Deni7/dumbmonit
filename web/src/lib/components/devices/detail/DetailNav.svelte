<script lang="ts">
	/** Back link of a monitor's page and its previous / next neighbours in the list order. */
	import { ChevronLeft, ChevronRight } from 'lucide-svelte';
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Neighbour {
		id: number;
		name: string;
	}

	let { around }: { around: { prev: Neighbour | null; next: Neighbour | null } } = $props();
</script>

<nav class="mb-4 flex items-center justify-between gap-3 text-sm max-sm:-mt-1 max-sm:mb-1" aria-label={m.w1monitors_detail_nav_aria()}>
	<a href={withBase('/targets')} class="inline-flex min-h-10 items-center gap-1 text-ink-2 hover:text-ink hover:underline sm:min-h-0">{m.devices_target_back()}</a>
	{#if around.prev || around.next}
		<span class="flex items-center gap-1">
			{#if around.prev}
				<a
					href={withBase(`/targets/${around.prev.id}`)}
					class="inline-flex min-h-10 min-w-10 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal sm:min-h-0 sm:size-8"
					aria-label={m.w1monitors_detail_prev({ name: around.prev.name })}
					title={m.w1monitors_detail_prev({ name: around.prev.name })}
					data-sveltekit-noscroll
				><ChevronLeft class="size-4" aria-hidden="true" /></a>
			{/if}
			{#if around.next}
				<a
					href={withBase(`/targets/${around.next.id}`)}
					class="inline-flex min-h-10 min-w-10 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal sm:min-h-0 sm:size-8"
					aria-label={m.w1monitors_detail_next({ name: around.next.name })}
					title={m.w1monitors_detail_next({ name: around.next.name })}
					data-sveltekit-noscroll
				><ChevronRight class="size-4" aria-hidden="true" /></a>
			{/if}
		</span>
	{/if}
</nav>
