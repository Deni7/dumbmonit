<script lang="ts">
	/**
	 * The dashboards' small glyphs (12 × 12, stroked in the current colour):
	 * the shape that goes with a status colour so it never stands alone. A
	 * cross is down, an exclamation mark degraded, two bars paused, a clock
	 * maintenance, a ringed tick acknowledged. Decorative unless `label` is
	 * given: the word beside it usually says the same.
	 */
	import type { GlyphName } from './plates.js';

	interface Props {
		name: GlyphName;
		label?: string;
		class?: string;
	}

	let { name, label, class: className = 'size-3' }: Props = $props();
</script>

<svg
	viewBox="0 0 12 12"
	class={`inline-block shrink-0 ${className}`}
	fill="none"
	stroke="currentColor"
	stroke-width="1.7"
	stroke-linecap="round"
	stroke-linejoin="round"
	role={label ? 'img' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : 'true'}
>
	{#if name === 'ok'}
		<path d="M3 6.4l2.1 2.1L9.2 3.8" />
	{:else if name === 'crit'}
		<path d="M3.6 3.6l4.8 4.8M8.4 3.6L3.6 8.4" />
	{:else if name === 'warn'}
		<path d="M6 2.6v4.3M6 9v.2" />
	{:else if name === 'info'}
		<path d="M6 5.4v3.8M6 2.8v.2" />
	{:else if name === 'paused'}
		<path d="M4.5 3.2v5.6M7.5 3.2v5.6" />
	{:else if name === 'maint'}
		<circle cx="6" cy="6" r="3.8" /><path d="M6 4v2.3l1.5 1" />
	{:else if name === 'run'}
		<path d="M4.2 3.2l4.6 2.8-4.6 2.8z" />
	{:else if name === 'ack'}
		<circle cx="6" cy="6" r="4.2" /><path d="M4.2 6.1l1.3 1.3 2.4-2.6" />
	{:else if name === 'up'}
		<path d="M6 9V3M3.5 5.5L6 3l2.5 2.5" />
	{:else if name === 'down'}
		<path d="M6 3v6M3.5 6.5L6 9l2.5-2.5" />
	{:else}
		<circle cx="6" cy="6" r="1.4" />
	{/if}
</svg>
