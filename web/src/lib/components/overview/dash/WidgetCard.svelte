<script lang="ts">
	/**
	 * The frame of one dashboard widget: title, a short subtitle, an optional
	 * right-side control, the body and optional footer links. Its width comes
	 * from the layout grid (`span`, 12 columns); below 720px every widget takes
	 * the full width. Shared by layouts A, B and C.
	 *
	 * `fill`: the card takes the height its parent gives it (the Hybrid layout,
	 * which never scrolls the page) and its body scrolls on its own; it places
	 * no column span, and the footer links move into the header.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		sub?: string;
		/** Columns out of 12 on wide screens. */
		span?: 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
		/** No inner padding (tables that run edge to edge). */
		flush?: boolean;
		/** Fill the parent's height, scroll inside (see above). */
		fill?: boolean;
		right?: Snippet;
		foot?: Snippet;
		children: Snippet;
		class?: string;
	}

	let { title, sub, span = 6, flush = false, fill = false, right, foot, children, class: cls = '' }: Props = $props();

	const SPAN: Record<number, string> = {
		2: 'xl:col-span-2 lg:col-span-6',
		3: 'xl:col-span-3 lg:col-span-6',
		4: 'xl:col-span-4 lg:col-span-6',
		5: 'xl:col-span-5 lg:col-span-6',
		6: 'lg:col-span-6',
		7: 'xl:col-span-7 lg:col-span-12',
		8: 'xl:col-span-8 lg:col-span-12',
		9: 'xl:col-span-9 lg:col-span-12',
		12: 'lg:col-span-12'
	};
	const place = $derived(fill ? 'min-h-0 overflow-hidden' : `col-span-12 ${SPAN[span]}`);
	const links = 'flex flex-wrap items-center gap-x-4 text-[0.8125rem] font-medium text-ink-2 [&_a]:inline-flex [&_a]:min-h-10 [&_a]:items-center [&_a:hover]:text-ink [&_a:hover]:underline';
</script>

<section class={`flex min-w-0 flex-col rounded-[var(--radius-card)] border border-line bg-surface ${place} ${cls}`}>
	<header class="flex shrink-0 flex-wrap items-center gap-x-2.5 gap-y-1.5 px-4 pt-3 pb-2.5">
		<h2 class="m-0 text-base font-semibold tracking-tight text-ink">{title}</h2>
		{#if sub}<span class="text-[0.8125rem] text-ink-3">{sub}</span>{/if}
		{#if right || (fill && foot)}
			<div class="ml-auto flex items-center gap-x-3 gap-y-1.5">
				{#if right}{@render right()}{/if}
				{#if fill && foot}<div class={`${links} [&_a]:-my-2`}>{@render foot()}</div>{/if}
			</div>
		{/if}
	</header>
	<div class={`min-w-0 flex-1 ${fill ? 'min-h-0 overflow-y-auto overscroll-contain [scrollbar-width:thin]' : ''} ${flush ? '' : 'px-4 pb-4'}`}>
		{@render children()}
	</div>
	{#if foot && !fill}
		<footer class={`${links} border-t border-line px-4`}>
			{@render foot()}
		</footer>
	{/if}
</section>
