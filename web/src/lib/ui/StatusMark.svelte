<script lang="ts">
	/**
	 * A small status mark whose SHAPE carries the state as much as its colour:
	 * a filled circle, a filled square, a triangle, a diamond or a hollow ring.
	 * Colour never stands alone. The label (text via props) is exposed as
	 * `aria-label` and `title`.
	 */
	interface Props {
		shape: 'circle' | 'square' | 'triangle' | 'diamond' | 'ring';
		tone: 'signal' | 'advisory' | 'warning' | 'ghost';
		label: string;
		title?: string;
		class?: string;
	}

	let { shape, tone, label, title, class: className = '' }: Props = $props();

	const COLOR: Record<Props['tone'], string> = {
		signal: 'text-signal',
		advisory: 'text-advisory',
		warning: 'text-warning',
		ghost: 'text-ghost'
	};
</script>

<svg
	class={`inline-block size-3.5 shrink-0 ${COLOR[tone]} ${className}`}
	viewBox="0 0 14 14"
	role="img"
	aria-label={label}
>
	<title>{title ?? label}</title>
	{#if shape === 'circle'}
		<circle cx="7" cy="7" r="4.5" fill="currentColor" />
	{:else if shape === 'square'}
		<rect x="3" y="3" width="8" height="8" rx="1.5" fill="currentColor" />
	{:else if shape === 'triangle'}
		<path d="M7 2.5 12 11.5H2Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round" />
	{:else if shape === 'diamond'}
		<path d="M7 2 12 7 7 12 2 7Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round" />
	{:else}
		<circle cx="7" cy="7" r="4" fill="none" stroke="currentColor" stroke-width="1.75" />
	{/if}
</svg>
