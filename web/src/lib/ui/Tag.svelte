<script lang="ts">
	/**
	 * A small, quiet label beside a value: the kind of a monitor ("PostgreSQL",
	 * "HTTP"). Static, never a control (see `Chip` for a filter toggle). It
	 * truncates within its `max-w-32` and shows the whole text as `title`.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
		/** Whole text, shown on hover when the tag is cut. */
		title?: string;
		class?: string;
	}

	let { children, title, class: className = '' }: Props = $props();
</script>

<span
	class={`inline-flex h-5 max-w-32 min-w-0 shrink-0 items-center rounded-[var(--radius-plate)] border border-line px-1.5 text-[0.6875rem] leading-none font-medium text-ink-3 ${className}`}
	{title}
>
	<span class="truncate">{@render children()}</span>
</span>
