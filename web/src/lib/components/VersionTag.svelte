<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The running version, a small line at the foot of the page (end of the
	 * layout, right-aligned, in the flow so it never covers content) —
	 * discreet on purpose: a build hash is for a bug report, not a headline.
	 * Mounted once by the root layout on every page except `/wall` (a kiosk
	 * display with nothing to spare at its edges). Hidden on phones, where the
	 * bottom tab bar already owns that corner; `Settings → About` carries the
	 * same figure there. Reading the version never requires a session: the
	 * health endpoint behind it is public.
	 */
	import { getHealth } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import SwissCross from '#lib/components/SwissCross.svelte';
	import UpdatePill from '#lib/components/UpdatePill.svelte';

	let { inline = false }: { inline?: boolean } = $props();
	let version = $state<{ number: string; build?: string } | null>(null);
	$effect(() => {
		const controller = new AbortController();
		getHealth(controller.signal)
			.then((health) => (version = { number: health.version, build: health.build }))
			.catch(() => {});
		return () => controller.abort();
	});
</script>

{#if version}
	<div class={inline ? 'inline-flex items-center gap-1' : 'mr-4 mb-3 hidden items-center gap-1 self-end sm:mr-6 sm:flex'}>
		<a
			href={withBase('/settings/system#about')}
			class={`tnum rounded-md px-1.5 py-0.5 text-[0.6875rem] font-medium transition-opacity hover:text-ink-2 hover:opacity-100 ${inline ? 'text-ink-3' : 'text-ink-3/70 opacity-70'}`}
			title={version.build ? `Version ${version.number}, build ${version.build}` : `Version ${version.number}`}
			>v{version.number}{#if version.build}<span class="font-mono"> · {version.build}</span>{/if}</a
		>
		<SwissCross class="size-3" />
	</div>
{/if}

<!-- The update check needs a session: asking before sign-in only logs a 401. -->
{#if !inline && auth.canUseApi}<UpdatePill />{/if}
