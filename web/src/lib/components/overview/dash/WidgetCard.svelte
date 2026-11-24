<script lang="ts">
	/**
	 * The frame of one overview panel, the same for all of them: a 48px header
	 * with the title (16px semibold, as the alerts page's panels; a link with a
	 * chevron when `href` is given) and an optional control on the right, then
	 * the body. The panel takes the height the grid gives it and its body
	 * scrolls on its own (on a narrow window the panels stack and the page
	 * scrolls). Rows draw their own hairline (`parts/scale.ts`), so the body
	 * has no padding: rows run edge to edge.
	 */
	import type { Snippet } from 'svelte';
	import { ChevronRight } from 'lucide-svelte';
	import { withBase } from '#lib/paths.js';

	interface Props {
		title: string;
		/** Path inside the app, without the base: the page with the whole list. */
		href?: string;
		right?: Snippet;
		children: Snippet;
		class?: string;
	}

	let { title, href, right, children, class: cls = '' }: Props = $props();
</script>

<section class={`flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[var(--radius-card)] border border-line bg-surface ${cls}`}>
	<header class="flex h-12 shrink-0 items-center gap-3 px-4">
		<h2 class="m-0 min-w-0 truncate text-base font-semibold tracking-tight text-ink">
			{#if href}
				<a href={withBase(href)} class="group inline-flex min-h-10 max-w-full items-center gap-1 hover:underline">
					<span class="truncate">{title}</span>
					<ChevronRight class="size-4 shrink-0 text-ink-3 transition-transform group-hover:translate-x-0.5" aria-hidden="true" />
				</a>
			{:else}
				{title}
			{/if}
		</h2>
		{#if right}
			<div class="ml-auto flex shrink-0 items-center gap-3 text-[0.8125rem] font-medium text-ink-2">{@render right()}</div>
		{/if}
	</header>
	<div class="min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-width:thin]">
		{@render children()}
	</div>
</section>
