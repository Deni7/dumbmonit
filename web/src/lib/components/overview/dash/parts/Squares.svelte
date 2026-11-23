<script lang="ts">
	/**
	 * One small mark per monitor, its shape carrying the status as well as its
	 * colour: a square (up), a circle (degraded), a diamond (down), a hollow
	 * square (paused). Each mark has the monitor's name as tooltip; the count
	 * beside the marks is what screen readers get.
	 */
	import type { WidgetStatus } from '#lib/api/index.js';
	import { monitorTitle } from './labels.js';

	interface Props {
		items: readonly { id: number; name: string; kind: string; status: WidgetStatus }[];
		/** Legend use: no tooltip. */
		decorative?: boolean;
		class?: string;
	}

	let { items, decorative = false, class: cls = '' }: Props = $props();

	const SHAPE: Record<WidgetStatus, string> = {
		up: 'rounded-[2px] bg-river-up',
		degraded: 'rounded-full bg-advisory',
		down: 'rotate-45 scale-[0.85] bg-warning',
		paused: 'rounded-[2px] border-[1.5px] border-ink-3 bg-transparent'
	};
</script>

<span class={`flex flex-wrap gap-0.5 ${cls}`} aria-hidden="true">
	{#each items as item (item.id)}
		<i class={`block size-[7px] shrink-0 ${SHAPE[item.status]}`} title={decorative ? undefined : monitorTitle(item)}></i>
	{/each}
</span>
