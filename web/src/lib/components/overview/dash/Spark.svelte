<script lang="ts">
	/**
	 * A sparkline that stretches to its box: optional soft area, optional dashed
	 * reference line (an objective). Decorative: the figure beside it carries the
	 * number, so it is hidden from screen readers unless `label` is given.
	 */
	interface Props {
		values: (number | null)[];
		height?: number;
		min?: number;
		max?: number;
		/** Dashed horizontal line at this value (e.g. an SLO). */
		ref?: number;
		area?: boolean;
		/** Stroke colour as a CSS colour, token based (`var(--color-signal)`…). */
		color?: string;
		label?: string;
	}

	let { values, height = 28, min, max, ref, area = true, color = 'var(--color-ink-2)', label }: Props = $props();
	const W = 100;
	const points = $derived(values.filter((v): v is number => v !== null && Number.isFinite(v)));
	const lo = $derived(min ?? (points.length ? Math.min(...points) : 0));
	const hi = $derived(max ?? (points.length ? Math.max(...points) : 1));
	function y(v: number): number {
		const span = hi - lo || 1;
		const c = Math.max(lo, Math.min(hi, v));
		return height - 2 - ((c - lo) / span) * (height - 4);
	}
	const path = $derived.by(() => {
		if (points.length < 2) return '';
		let d = '';
		let started = false;
		values.forEach((v, i) => {
			if (v === null || !Number.isFinite(v)) {
				started = false;
				return;
			}
			const x = (i / (values.length - 1)) * W;
			d += `${started ? 'L' : 'M'}${x.toFixed(2)} ${y(v).toFixed(2)}`;
			started = true;
		});
		return d;
	});
</script>

<svg
	viewBox={`0 0 ${W} ${height}`}
	preserveAspectRatio="none"
	class="block w-full"
	style={`height:${height}px`}
	role={label ? 'img' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : 'true'}
>
	{#if path}
		{#if area}<path d={`${path}L${W} ${height}L0 ${height}Z`} fill={color} opacity="0.12" />{/if}
		{#if ref !== undefined}
			<line x1="0" x2={W} y1={y(ref)} y2={y(ref)} stroke="var(--color-ink-3)" stroke-dasharray="3 3" vector-effect="non-scaling-stroke" />
		{/if}
		<path d={path} fill="none" stroke={color} stroke-width="1.6" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
	{:else}
		<line x1="0" x2={W} y1={height / 2} y2={height / 2} stroke="var(--color-line-strong)" stroke-dasharray="2 4" vector-effect="non-scaling-stroke" />
	{/if}
</svg>
