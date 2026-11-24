<script lang="ts">
	/**
	 * The gate around the pages a link opens (`/reset`, `/invite`): the same
	 * panel as sign-in and first setup, over the dot field, with the pigeon on
	 * the corner.
	 */
	import type { Snippet } from 'svelte';
	import { DotField } from '#lib/ui/index.js';
	import Logo from '#lib/components/Logo.svelte';
	import Mascot from '#lib/components/Mascot.svelte';
	import LanguageSelect from '#lib/components/LanguageSelect.svelte';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		children: Snippet;
		mood?: 'watch' | 'happy' | 'dizzy';
	}

	let { children, mood = 'watch' }: Props = $props();
</script>

<div class="relative min-h-screen overflow-hidden bg-canvas">
	<DotField opacity={1} dotSpacing={13} />

	<div class="absolute top-3 right-3 z-20 flex items-center gap-1 sm:top-4 sm:right-4">
		<label for="gate-language" class="sr-only">{m["settings.appearance.language_label"]()}</label>
		<LanguageSelect id="gate-language" class="max-w-36 border-transparent bg-transparent" />
	</div>

	<main class="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 py-12">
		<section class="gate-panel rise-in relative w-full max-w-sm rounded-[var(--radius-card)] border border-line p-6 shadow-float sm:p-7" aria-labelledby="gate-title">
			<Mascot {mood} class="gate-mascot pointer-events-none absolute -top-12 -right-4 size-20 rotate-6 sm:-top-16 sm:-right-6 sm:size-28" />
			<div class="flex items-center gap-2.5">
				<Logo class="size-8" />
				<span class="text-[1.05rem] font-bold tracking-tight text-ink">DumbMonit</span>
			</div>
			{@render children()}
		</section>

		<p class="mt-6 text-[0.8125rem] text-ink-2">{m.auth_footer()}</p>
	</main>
</div>

<style>
	:global(.gate-mascot) {
		filter: drop-shadow(0 10px 18px rgb(0 0 0 / 0.18));
	}

	.gate-panel {
		background:
			linear-gradient(180deg, rgb(255 255 255 / 0.04), rgb(0 0 0 / 0.02)),
			repeating-linear-gradient(180deg, transparent 0 3px, rgb(127 127 127 / 0.025) 3px 4px),
			color-mix(in srgb, var(--c-surface) 95%, transparent);
		backdrop-filter: blur(10px);
		-webkit-backdrop-filter: blur(10px);
	}
</style>
