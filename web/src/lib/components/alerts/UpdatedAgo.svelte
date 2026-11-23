<script lang="ts">
	/**
	 * A discreet "Updated 12 s ago", with a refresh button when `onrefresh` is
	 * given (the overview has none: polling is enough). It ticks on its own
	 * clock so the figure moves between two polls, and says so plainly (a word,
	 * not only a colour) when several polls in a row have failed.
	 */
	import { RefreshCw } from 'lucide-svelte';
	import { formatRelative } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { isStale } from './facts';

	interface Props {
		/** When the data was last fetched successfully (ms since epoch). */
		updatedAt: number | null;
		onrefresh?: () => Promise<void> | void;
	}

	let { updatedAt, onrefresh }: Props = $props();

	let now = $state(Date.now());
	let busy = $state(false);
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 5000);
		return () => clearInterval(timer);
	});

	const stale = $derived(isStale(updatedAt, now));
	// `now` is read so the text moves with the ticker.
	const when = $derived.by(() => {
		void now;
		return updatedAt === null ? '' : formatRelative(new Date(updatedAt));
	});

	async function refresh() {
		if (!onrefresh) return;
		busy = true;
		try {
			await onrefresh();
		} finally {
			busy = false;
		}
	}
</script>

{#if updatedAt !== null}
	<p class={`flex items-center gap-1 text-[0.8125rem] ${stale ? 'text-warning-ink' : 'text-ink-3'}`}>
		<span class="tnum" role="status">
			{stale ? m.alerts_updated_stale({ when }) : m.alerts_updated({ when })}
		</span>
		{#if onrefresh}
		<button
			type="button"
			class="inline-flex size-10 items-center justify-center rounded-lg text-ink-3 transition-colors hover:bg-surface-2 hover:text-ink disabled:opacity-60 sm:size-8"
			aria-label={m.alerts_updated_refresh()}
			title={m.alerts_updated_refresh()}
			disabled={busy}
			onclick={() => void refresh()}
		>
			<RefreshCw class={`size-3.5 ${busy ? 'animate-spin' : ''}`} aria-hidden="true" />
		</button>
		{/if}
	</p>
{/if}
