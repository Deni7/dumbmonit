<script lang="ts">
	/**
	 * One series of a history test: the measured values over the window (the
	 * shared `Sparkline`, threshold dashed) and, right under it on the same
	 * width, a strip with a block for each period the rule would have been
	 * firing. Both scale together, so a block sits under the bump that caused
	 * it. The periods are also listed in words next to it by the caller: the
	 * strip is never the only way to read them.
	 */
	import type { BacktestPeriod } from '#lib/api/index.js';
	import { Sparkline, TONE_COLOR } from '#lib/viz/index.js';

	interface Props {
		values: number[];
		periods: BacktestPeriod[];
		/** Window, Unix seconds. */
		start: number;
		end: number;
		threshold: number;
		/** Accessible name of the value line. */
		label: string;
		format?: (value: number) => string;
	}

	let { values, periods, start, end, threshold, label, format }: Props = $props();

	const WIDTH = 280;
	const STRIP = 8;
	const span = $derived(Math.max(1, end - start));
	const blocks = $derived(
		periods.map((period) => {
			const from = Math.min(Math.max(period.start, start), end);
			const to = Math.min(Math.max(period.end ?? end, from), end);
			const x = ((from - start) / span) * WIDTH;
			return { key: period.start, x, width: Math.max(2, ((to - from) / span) * WIDTH) };
		})
	);
</script>

<div class="grid w-full max-w-[280px] gap-1">
	<Sparkline {values} width={WIDTH} height={32} tone="warning" {threshold} {label} {format} showLast={false} />
	<svg width={WIDTH} height={STRIP} viewBox={`0 0 ${WIDTH} ${STRIP}`} class="block max-w-full" aria-hidden="true">
		<rect x="0" y={STRIP / 2 - 0.5} width={WIDTH} height="1" fill="var(--c-line-strong)" />
		{#each blocks as block (block.key)}
			<rect x={Math.min(block.x, WIDTH - block.width)} y="0" width={block.width} height={STRIP} rx="1.5" fill={TONE_COLOR.warning} />
		{/each}
	</svg>
</div>
