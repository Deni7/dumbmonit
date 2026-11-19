<script lang="ts">
	/**
	 * Where a monitor sits, as a minimal pill: the two-letter code of its
	 * location ("CR" for Crissier). The full name is the tooltip and the
	 * accessible name, so the code never has to carry it alone.
	 */
	import { m } from '#lib/paraglide/messages.js';

	let { code, name, class: className = '' }: { code: string; name: string; class?: string } = $props();
</script>

<span
	class={`inline-flex h-5 min-w-7 shrink-0 items-center justify-center rounded-md bg-surface-2 px-1 text-[0.6875rem] leading-none font-semibold tracking-wide text-ink-2 ${className}`}
	title={m.devices_site_badge_aria({ name })}
>
	<span aria-hidden="true">{code}</span>
	<span class="sr-only">{m.devices_site_badge_aria({ name })}</span>
</span>
