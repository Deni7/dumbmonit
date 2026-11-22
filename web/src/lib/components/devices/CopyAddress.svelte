<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * A small icon button that copies a monitor's address and says so (the icon
	 * turns into a check for a moment, and a polite status line tells screen
	 * readers). 40 px tap target, quiet until hovered.
	 */
	import { Check, Copy } from 'lucide-svelte';
	import { copyText } from '#lib/clipboard.js';

	interface Props {
		value: string;
		class?: string;
	}

	let { value, class: className = '' }: Props = $props();

	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		copied = await copyText(value);
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1600);
	}

	$effect(() => () => clearTimeout(timer));

	const label = $derived(copied ? m.app_ui_copied() : m.w1monitors_copy_address());
</script>

<button
	type="button"
	class={`inline-flex size-10 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal sm:-my-2.5 sm:size-8 ${className}`}
	onclick={copy}
	aria-label={label}
	title={label}
>
	{#if copied}<Check class="size-4 text-signal-ink" aria-hidden="true" />{:else}<Copy class="size-4" aria-hidden="true" />{/if}
</button>
<span class="sr-only" role="status" aria-live="polite">{copied ? m.app_ui_copied() : ''}</span>
