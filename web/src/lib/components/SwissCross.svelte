<script lang="ts">
	/**
	 * The Swiss cross: a white cross on a red tile. A brand mark, so the red is
	 * the same in light and dark (the one assumed exception to the colour
	 * tokens, kept in `--swiss-red` below). On a 32 grid the cross is 20 wide
	 * and tall, its arms 6 thick, centred.
	 */
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		class?: string;
	}
	let { class: className = 'size-3.5' }: Props = $props();
</script>

<svg viewBox="0 0 32 32" class={`swiss-cross shrink-0 ${className}`} role="img" aria-label={m.brand_swiss_made()}>
	<title>{m.brand_swiss_made()}</title>
	<rect width="32" height="32" rx="3" class="tile" />
	<path d="M13 6h6v7h7v6h-7v7h-6v-7H6v-6h7z" class="cross" />
</svg>

<style>
	.swiss-cross {
		--swiss-red: #da291c;
		--swiss-white: #fff;
	}
	.tile {
		fill: var(--swiss-red);
	}
	.cross {
		fill: var(--swiss-white);
	}
</style>
