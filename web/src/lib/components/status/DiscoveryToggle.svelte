<script lang="ts">
	/**
	 * Whether the page appears in the server's list of public pages
	 * (`GET /api/public/status`), where website plugins such as the WordPress
	 * banner offer a choice of pages. Only a published page anyone may read is
	 * ever listed: the help line says so when the page is a draft or private.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { Toggle } from '#lib/ui/index.js';

	interface Props {
		listed: boolean;
		published: boolean;
		/** The access mode is `public` (no password, no accounts). */
		open: boolean;
		idPrefix: string;
		disabled?: boolean;
	}

	let { listed = $bindable(true), published, open, idPrefix, disabled = false }: Props = $props();

	const help = $derived(
		!published
			? m.w8wp_discovery_unpublished()
			: !open
				? m.w8wp_discovery_private()
				: listed
					? m.w8wp_discovery_on()
					: m.w8wp_discovery_off()
	);
</script>

<div class="flex min-h-10 items-center justify-between gap-3 rounded-lg border border-line px-3 py-1.5">
	<div class="min-w-0">
		<label for="{idPrefix}-listed" class="block text-sm font-semibold text-ink">{m.w8wp_discovery_label()}</label>
		<p class="text-[0.8125rem] text-ink-2">{help}</p>
	</div>
	<Toggle id="{idPrefix}-listed" bind:checked={listed} {disabled} label={m.w8wp_discovery_label()} />
</div>
