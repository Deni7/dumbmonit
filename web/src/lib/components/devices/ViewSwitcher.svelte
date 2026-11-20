<script lang="ts">
	/**
	 * Compact / tiles for /targets: two toggle buttons in one strip, drawn
	 * exactly as the "Severity / Monitor" switch of the Alerts list (32 px
	 * from `sm`, 40 px on phones). Each button carries its name (a word from
	 * `sm`, an icon on phones, read out everywhere) and `aria-pressed` says
	 * which one is on.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { LayoutGrid, List } from 'lucide-svelte';
	import type { RackView } from './view';

	interface Props {
		value: RackView;
		onchange: (value: RackView) => void;
	}

	let { value, onchange }: Props = $props();

	const OPTIONS = $derived([
		{ id: 'compact' as const, label: m.devices_targets_view_compact(), icon: List },
		{ id: 'tiles' as const, label: m.devices_targets_view_tiles(), icon: LayoutGrid }
	]);
</script>

<!-- The very strip of the Alerts "Severity / Monitor" switch: same frame, height and type; an icon alone on phones. -->
<div role="group" aria-label={m.devices_targets_view_label()} class="inline-flex shrink-0 rounded-lg border border-line bg-surface-2 p-0.5">
	{#each OPTIONS as option (option.id)}
		{@const active = option.id === value}
		{@const Icon = option.icon}
		<button
			type="button"
			aria-pressed={active}
			title={option.label}
			onclick={() => onchange(option.id)}
			class={`inline-flex h-10 min-w-10 items-center justify-center rounded-md px-3 text-[0.8125rem] font-semibold whitespace-nowrap transition max-sm:px-0 sm:h-8 ${active ? 'bg-surface text-ink shadow-lift' : 'text-ink-2 hover:text-ink'}`}
		>
			<Icon class="size-4 sm:hidden" aria-hidden="true" />
			<span class="max-sm:sr-only">{option.label}</span>
		</button>
	{/each}
</div>
