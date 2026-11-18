<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The monitor page's Settings button: a gear that opens the day-to-day
	 * controls (enable or disable, snooze with its durations, logs), then Delete
	 * apart and in the danger tone, then the real Settings page at the foot.
	 * The gear is tinted while a snooze window covers the monitor, so the state
	 * is still visible with the menu closed.
	 */
	import { Power, PowerOff, ScrollText, Settings } from 'lucide-svelte';
	import type { Silence, Target } from '#lib/api/index.js';
	import { listSilences } from '#lib/api/index.js';
	import SilenceControl from '#lib/components/devices/SilenceControl.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { Button, Confirm, Menu } from '#lib/ui/index.js';

	interface Props {
		target: Target;
		/** Bumped by the page on each refresh so the tint follows the server. */
		refreshKey?: number;
		savingEnabled?: boolean;
		deleting?: boolean;
		onsetenabled: (value: boolean) => void;
		ondelete: () => unknown;
		/** An agent's monitor first asks whether to take the agent off the machine. */
		onasktoremove: () => void;
	}

	let { target, refreshKey = 0, savingEnabled = false, deleting = false, onsetenabled, ondelete, onasktoremove }: Props = $props();

	let silences = $state<Silence[]>([]);
	$effect(() => {
		void target.id;
		void refreshKey;
		const controller = new AbortController();
		listSilences(controller.signal)
			.then((list) => (silences = list))
			.catch(() => {});
		return () => controller.abort();
	});
	const silenced = $derived(silences.some((s) => s.active_now && (s.target_id === target.id || s.target_id === null)));

	const item =
		'flex min-h-10 w-full items-center gap-2 rounded-lg px-2 text-left text-sm font-semibold text-ink hover:bg-surface hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal';
</script>

<Menu label={m.devices_target_settings()} align="right">
	{#snippet trigger({ toggle, open })}
		<button
			type="button"
			class={`flex size-10 items-center justify-center rounded-lg border hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal ${
				silenced ? 'border-advisory/50 bg-advisory-soft text-advisory-ink' : 'border-line text-ink-2 hover:bg-surface-2'
			}`}
			onclick={toggle}
			aria-haspopup="menu"
			aria-expanded={open}
			aria-label={m.devices_target_settings()}
			title={m.devices_target_settings()}
		>
			<Settings class="size-4" aria-hidden="true" />
		</button>
	{/snippet}
	{#snippet children({ close })}
		<div class="flex w-64 max-w-full flex-col items-stretch gap-0.5">
			<button type="button" role="menuitem" class={item} disabled={savingEnabled} onclick={() => { onsetenabled(!target.enabled); close(); }}>
				{#if target.enabled}
					<PowerOff class="size-4 text-ink-2" aria-hidden="true" />{m.devices_target_disable()}
				{:else}
					<Power class="size-4 text-ink-2" aria-hidden="true" />{m.devices_target_enable()}
				{/if}
			</button>

			{#if target.enabled}
				<div class="mt-1 border-t border-line pt-1">
					<p class="m-0 px-2 py-1 text-sm font-semibold text-ink-2">{m.devices_silence_menu()}</p>
					<SilenceControl {target} {refreshKey} inline />
				</div>
			{/if}

			<div class="mt-1 border-t border-line pt-1">
				<a href={withBase(`/targets?tab=logs&target=${target.id}`)} role="menuitem" class={item}>
					<ScrollText class="size-4 text-ink-2" aria-hidden="true" />{m.devices_target_logs()}
				</a>
			</div>

			<div class="mt-1 border-t border-line pt-1">
				{#if target.kind === 'agent'}
					<Button variant="danger" size="md" class="w-full justify-start" onclick={() => { onasktoremove(); close(); }}>{m.devices_target_delete()}</Button>
				{:else}
					<Confirm size="md" variant="danger" class="w-full justify-start" confirmLabel={m.devices_target_delete_confirm()} onconfirm={ondelete} loading={deleting}>{m.devices_target_delete()}</Confirm>
				{/if}
			</div>

			<div class="mt-1 border-t border-line pt-1">
				<a href={withBase(`/targets/${target.id}/edit`)} role="menuitem" class={item}>
					<Settings class="size-4 text-ink-2" aria-hidden="true" />{m.devices_target_settings()}
				</a>
			</div>
		</div>
	{/snippet}
</Menu>
