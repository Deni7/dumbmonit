<script lang="ts">
	/**
	 * Advice under a new-password field: four bars and a word. The word, not
	 * the colour, carries the level. It never blocks the form.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { passwordStrength } from '#lib/password-strength.js';

	let { value }: { value: string } = $props();

	const level = $derived(passwordStrength(value));
	const label = $derived(
		[m.w2global_strength_very_weak, m.w2global_strength_weak, m.w2global_strength_fair, m.w2global_strength_good, m.w2global_strength_strong][level]()
	);
	const TONE = ['bg-warning', 'bg-warning', 'bg-advisory', 'bg-signal', 'bg-signal'];
</script>

{#if value}
	<div class="mt-2 flex items-center gap-3" role="status">
		<div class="flex flex-1 gap-1" aria-hidden="true">
			{#each [1, 2, 3, 4] as step (step)}
				<span class={`h-1.5 flex-1 rounded-full ${step <= level ? TONE[level] : 'bg-line'}`}></span>
			{/each}
		</div>
		<span class="shrink-0 text-[0.8125rem] font-medium text-ink-2">{m.w2global_strength_label({ level: label })}</span>
	</div>
{/if}
