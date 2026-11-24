<script lang="ts">
	/**
	 * A small status mark: the same circle for every state, coloured by its
	 * tone, with a glyph cut out of it so colour never stands alone — nothing
	 * (up), a cross (down), an exclamation mark (misconfigured), a dot
	 * (pending), or the circle left hollow (disabled, unknown). The label
	 * (text via props) is exposed as `aria-label` and `title`.
	 */
	interface Props {
		glyph: 'none' | 'cross' | 'bang' | 'dot' | 'ring';
		tone: 'signal' | 'advisory' | 'warning' | 'ghost';
		label: string;
		title?: string;
		class?: string;
	}

	let { glyph, tone, label, title, class: className = '' }: Props = $props();

	const COLOR: Record<Props['tone'], string> = {
		signal: 'text-signal',
		advisory: 'text-advisory',
		warning: 'text-warning',
		// The ghost tone is a wash for empty cells: as a hollow ring it would vanish.
		ghost: 'text-ink-3'
	};
</script>

<svg
	class={`inline-block size-3.5 shrink-0 ${COLOR[tone]} ${className}`}
	viewBox="0 0 14 14"
	role="img"
	aria-label={label}
>
	<title>{title ?? label}</title>
	{#if glyph === 'ring'}
		<circle cx="7" cy="7" r="5.25" fill="none" stroke="currentColor" stroke-width="1.5" />
	{:else}
		<circle cx="7" cy="7" r="6" fill="currentColor" />
		<!-- The glyph in the surface colour: a cut-out, legible on every tone and theme. -->
		{#if glyph === 'cross'}
			<path d="M4.75 4.75 9.25 9.25M9.25 4.75 4.75 9.25" fill="none" stroke="var(--c-surface)" stroke-width="1.5" stroke-linecap="round" />
		{:else if glyph === 'bang'}
			<path d="M7 3.9V7.6" fill="none" stroke="var(--c-surface)" stroke-width="1.5" stroke-linecap="round" />
			<circle cx="7" cy="9.9" r="0.95" fill="var(--c-surface)" />
		{:else if glyph === 'dot'}
			<circle cx="7" cy="7" r="1.75" fill="var(--c-surface)" />
		{/if}
	{/if}
</svg>
