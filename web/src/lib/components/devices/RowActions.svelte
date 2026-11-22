<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * The quick actions of one monitor row, in an overflow menu: check now,
	 * enable or disable, copy the address. Revealed on hover or focus for a
	 * mouse and always there on touch (no hover to reveal it), never in the way
	 * of the row's own link (it sits above the stretched link). A press here
	 * never starts a drag of the row.
	 */
	import { Activity, Copy, EllipsisVertical, Power, PowerOff } from 'lucide-svelte';
	import type { Target } from '#lib/api/index.js';
	import { copyText } from '#lib/clipboard.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Menu } from '#lib/ui/index.js';

	export interface RowActionHandlers {
		/** Checks the monitor right now. */
		probe: (target: Target) => void;
		/** Turns the monitor on or off. */
		setEnabled: (target: Target, enabled: boolean) => void;
	}

	interface Props {
		target: Target;
		actions: RowActionHandlers;
	}

	let { target, actions }: Props = $props();

	let open = $state(false);
	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => () => clearTimeout(timer));

	/** An agent's address is its identity, not where it is reached: nothing to copy. */
	const copyable = $derived(target.kind !== 'agent' && target.kind !== 'agent_service' && target.address.trim() !== '');

	async function copy(close: () => void) {
		copied = await copyText(target.address);
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1600);
		close();
	}

	const item =
		'flex min-h-10 w-full items-center gap-2 rounded-lg px-2 text-left text-sm font-semibold text-ink hover:bg-surface hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal';
</script>

<span
	class="relative z-[1] flex items-center justify-center"
	onpointerdown={(e) => e.stopPropagation()}
	onkeydown={(e) => e.stopPropagation()}
	role="presentation"
>
	<Menu label={m.w1monitors_row_actions({ name: target.name })} align="right" bind:open>
		{#snippet trigger({ toggle, open: expanded })}
			<button
				type="button"
				class={`flex size-10 items-center justify-center rounded-lg text-ink-2 transition-opacity hover:bg-surface-2 hover:text-ink focus-visible:opacity-100 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal group-hover/row:opacity-100 group-focus-within/row:opacity-100 [@media(hover:hover)]:opacity-0 ${expanded ? '[@media(hover:hover)]:!opacity-100' : ''}`}
				onclick={toggle}
				aria-haspopup="menu"
				aria-expanded={expanded}
				aria-label={m.w1monitors_row_actions({ name: target.name })}
				title={m.w1monitors_row_actions({ name: target.name })}
			>
				<EllipsisVertical class="size-4" aria-hidden="true" />
			</button>
		{/snippet}
		{#snippet children({ close })}
			<div class="flex w-56 max-w-full flex-col items-stretch gap-0.5">
				{#if target.enabled}
					<button type="button" role="menuitem" class={item} onclick={() => { close(); actions.probe(target); }}>
						<Activity class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.devices_target_probe_now()}
					</button>
				{/if}
				{#if auth.isAdmin}
					<button type="button" role="menuitem" class={item} onclick={() => { close(); actions.setEnabled(target, !target.enabled); }}>
						{#if target.enabled}
							<PowerOff class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.devices_target_disable()}
						{:else}
							<Power class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.devices_target_enable()}
						{/if}
					</button>
				{/if}
				{#if copyable}
					<button type="button" role="menuitem" class={item} onclick={() => copy(close)}>
						<Copy class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.w1monitors_copy_address()}
					</button>
				{/if}
			</div>
		{/snippet}
	</Menu>
	<span class="sr-only" role="status" aria-live="polite">{copied ? m.app_ui_copied() : ''}</span>
</span>
