<script lang="ts">
	/**
	 * The key of a fleet view: one swatch per status, each with its glyph and
	 * its word, in the shape the view uses (a hexagon cell, or a square).
	 */
	import type { WidgetStatus } from '#lib/api/index.js';
	import { STATUS_FILL, STATUS_GLYPH } from './sites.js';
	import { statusWord } from './labels.js';
	import Squares from './Squares.svelte';

	interface Props {
		shape?: 'cell' | 'square';
		class?: string;
	}

	let { shape = 'cell', class: cls = '' }: Props = $props();
	const ORDER: WidgetStatus[] = ['up', 'degraded', 'down', 'paused'];
</script>

<ul class={`m-0 flex list-none flex-wrap gap-x-3 gap-y-1.5 p-0 text-xs text-ink-2 ${cls}`}>
	{#each ORDER as status (status)}
		<li class="inline-flex items-center gap-1.5">
			{#if shape === 'square'}
				<Squares items={[{ id: 0, name: '', kind: '', status }]} decorative />
			{:else}
				<span class="inline-grid size-3 place-items-center rounded-[3px] text-[0.5625rem] leading-none font-extrabold text-surface" style={`background:${STATUS_FILL[status]}`} aria-hidden="true">{STATUS_GLYPH[status]}</span>
			{/if}
			{statusWord(status)}
		</li>
	{/each}
</ul>
