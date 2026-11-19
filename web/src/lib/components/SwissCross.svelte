<script lang="ts">
	/**
	 * The Swiss cross: a white cross on a red disc. A brand mark, so the red is
	 * the same in light and dark (the one assumed exception to the colour
	 * tokens, kept in `--swiss-red` below). On a 32 grid the cross is 17 wide
	 * and tall, its arms 5 thick, centred in the disc.
	 */
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		class?: string;
	}
	let { class: className = 'size-3.5' }: Props = $props();
</script>

<svg viewBox="0 0 32 32" class={`swiss-cross shrink-0 ${className}`} shape-rendering="geometricPrecision" role="img" aria-label={m.brand_swiss_made()}>
	<title>{m.brand_swiss_made()}</title>
	<circle cx="16" cy="16" r="16" class="tile" />
	<path d="M13.5 7.5h5v6h6v5h-6v6h-5v-6h-6v-5h6z" class="cross" />
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
