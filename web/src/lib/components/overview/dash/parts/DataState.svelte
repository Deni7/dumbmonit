<script lang="ts">
	/**
	 * The three moments of a widget body before its content: reading (a
	 * skeleton of the same height), failed (one line), and empty (one honest
	 * sentence). Figures already shown stay up when a later refresh fails.
	 */
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { Skeleton } from '#lib/ui/index.js';

	interface Props {
		loading: boolean;
		error: unknown;
		empty: boolean;
		emptyText: string;
		rows?: number;
		/** Padding around the placeholder lines (for flush cards). */
		class?: string;
		children: Snippet;
	}

	let { loading, error, empty, emptyText, rows = 3, class: cls = '', children }: Props = $props();
</script>

{#if loading}
	<div class={`flex flex-col gap-2.5 py-1 ${cls}`} aria-busy="true">
		<Skeleton class="h-4 w-full" {rows} />
	</div>
{:else if error}
	<p class={`m-0 py-2 text-[0.8125rem] font-medium text-warning-ink ${cls}`} role="alert">{m.w9layout_state_error()}</p>
{:else if empty}
	<p class={`m-0 py-2 text-[0.8125rem] text-ink-2 ${cls}`}>{emptyText}</p>
{:else}
	{@render children()}
{/if}
