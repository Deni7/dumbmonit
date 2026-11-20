<script lang="ts">
	/**
	 * The column names of the monitors table, once over the whole list (not in
	 * every folder): a quiet line on the page, no band, on the very grid of the
	 * rows (`columns.ts`). Its own container query, with the same 1 px sides as
	 * the cards, so it folds with them; with checkboxes on (selection), the
	 * same 36 px slot in front.
	 *
	 * Never cut: the short cells take their word whole, past their track if
	 * need be (the gap and the narrow centred cell beside leave room), and the
	 * security column says itself with the shield its grades carry.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { ShieldCheck } from 'lucide-svelte';
	import { HEADER_HEIGHT, SHOW, gridFor } from './columns';

	interface Props {
		/** The table has a site column (several locations). */
		showSite?: boolean;
		/** Rows carry a checkbox in front (selection mode). */
		selecting?: boolean;
		class?: string;
	}

	let { showSite = true, selecting = false, class: className = '' }: Props = $props();

	const grid = $derived(gridFor(showSite));
</script>

<div class={`@container border-x border-transparent ${className}`} aria-hidden="true">
	<div class="flex items-center gap-1.5 text-[0.75rem] font-medium whitespace-nowrap text-ink-3">
		{#if selecting}<span class="w-9 shrink-0"></span>{/if}
		<div class="min-w-0 flex-1">
			<div class={`${grid} ${HEADER_HEIGHT}`}>
				<span></span>
				<span class="truncate">{m.devices_cols_name()}</span>
				{#if showSite}<span class={`${SHOW.site} justify-center`}>{m.devices_cols_site()}</span>{/if}
				<span class={`${SHOW.river}`}>{m.devices_cols_last24h()}</span>
				<span class={`${SHOW.security} justify-center`} title={m.devices_cols_security()}>
					<ShieldCheck class="size-3.5 shrink-0" aria-hidden="true" />
				</span>
				<span class="flex justify-end">{m.devices_cols_checked()}</span>
			</div>
		</div>
	</div>
</div>
