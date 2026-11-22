<script lang="ts">
	/** Quiet strip at the foot of the screen while the server cannot be reached. */
	import { WifiOff } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { connection } from '#lib/stores/connection.svelte.js';
</script>

<div role="status" aria-live="polite" class="pointer-events-none fixed inset-x-0 bottom-0 z-[60] flex justify-center px-4 pb-[calc(1rem+var(--safe-bottom,0px))] print:hidden">
	{#if connection.lost}
		<p class="pointer-events-auto flex max-w-md items-center gap-2 rounded-lg border border-warning/50 bg-surface px-3 py-2 text-[0.8125rem] text-ink shadow-float">
			<WifiOff class="size-4 shrink-0 text-warning-ink" aria-hidden="true" />
			<span>{m.w2global_connection_lost()}</span>
		</p>
	{/if}
</div>
