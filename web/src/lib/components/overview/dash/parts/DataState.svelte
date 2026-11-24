<script lang="ts">
	/**
	 * The three moments of a panel body before its content: reading (a
	 * skeleton of the same height), failed (one line), and empty (one honest
	 * sentence), all with the same padding. Figures already shown stay up when
	 * a later refresh fails.
	 */
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { Skeleton } from '#lib/ui/index.js';
	import { STATE_PAD, TEXT_SUB } from './scale.js';

	interface Props {
		loading: boolean;
		error: unknown;
		empty: boolean;
		emptyText: string;
		rows?: number;
		children: Snippet;
	}

	let { loading, error, empty, emptyText, rows = 3, children }: Props = $props();
</script>

{#if loading}
	<div class={`flex flex-col gap-2.5 border-t border-line ${STATE_PAD}`} aria-busy="true">
		<Skeleton class="h-4 w-full" {rows} />
	</div>
{:else if error}
	<p class={`m-0 border-t border-line font-medium text-warning-ink ${STATE_PAD} text-[0.8125rem]`} role="alert">{m.w9layout_state_error()}</p>
{:else if empty}
	<p class={`m-0 border-t border-line ${STATE_PAD} ${TEXT_SUB}`}>{emptyText}</p>
{:else}
	{@render children()}
{/if}
