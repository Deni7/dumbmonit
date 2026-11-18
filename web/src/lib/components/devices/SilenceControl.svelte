<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * Quiet this device from one icon button and its small menu: "1 h" creates
	 * a one-off window starting now; "Until…" unfolds a picker for a custom end.
	 * While a window covers the device, the icon turns advisory and the menu
	 * says "Snoozed until 14:00" with a two-step "Lift" that deletes it.
	 */
	import type { Silence, Target } from '#lib/api/index.js';
	import { createSilence, deleteSilence, listSilences } from '#lib/api/index.js';
	import { Button, Confirm, DateTimeField, Menu } from '#lib/ui/index.js';
	import { BellOff, BellRing, CalendarClock } from 'lucide-svelte';
	import { formatDateTime } from '#lib/format.js';
	import { datetimeLocalIn, quickSilencePayload, scheduleLabel } from '#lib/components/alerts/helpers.js';

	interface Props {
		target: Target;
		/** Bumped by the page on each refresh so the icon follows the server. */
		refreshKey?: number;
		/** Only the options, no button and no menu of its own: for a menu that holds them among others. */
		inline?: boolean;
	}

	let { target, refreshKey = 0, inline = false }: Props = $props();

	let silences = $state<Silence[]>([]);
	let picking = $state(false);
	let until = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let menuOpen = $state(false);

	/** Windows covering this device right now: its own, then the global ones. */
	const own = $derived(silences.find((s) => s.active_now && s.target_id === target.id) ?? null);
	const global = $derived(silences.find((s) => s.active_now && s.target_id === null) ?? null);

	async function load(signal?: AbortSignal) {
		try {
			silences = await listSilences(signal);
		} catch {
			// The icon is a convenience: a failed read leaves the menu available.
		}
	}

	$effect(() => {
		void target.id;
		void refreshKey;
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	// A closed menu forgets its half-filled picker.
	$effect(() => {
		if (!inline && !menuOpen) {
			picking = false;
			error = null;
		}
	});

	function untilLabel(silence: Silence): string {
		if (silence.schedule.kind === 'recovery') return m.devices_silence_until_recovery();
		return silence.schedule.kind === 'once'
			? m.devices_silence_until({ when: formatDateTime(silence.schedule.ends_at) })
			: `· ${scheduleLabel(silence.schedule)}`;
	}

	function openPicker() {
		until = datetimeLocalIn();
		error = null;
		picking = true;
		queueMicrotask(() => document.getElementById('silence-until')?.focus());
	}

	async function silenceFor(endsAt: Date) {
		busy = true;
		error = null;
		try {
			await createSilence({ ...quickSilencePayload(target, endsAt), name: m.devices_silence_name({ name: target.name }) });
			menuOpen = false;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : m.devices_silence_err_create();
		} finally {
			busy = false;
		}
	}

	function silenceOneHour() {
		return silenceFor(new Date(Date.now() + 3600_000));
	}

	function submitPicker(event: SubmitEvent) {
		event.preventDefault();
		const end = new Date(until);
		if (Number.isNaN(end.getTime())) {
			error = m.devices_silence_err_pick();
			return;
		}
		if (end.getTime() <= Date.now() + 60_000) {
			error = m.devices_silence_err_future();
			return;
		}
		void silenceFor(end);
	}

	async function lift(silence: Silence) {
		busy = true;
		error = null;
		try {
			await deleteSilence(silence.id);
			menuOpen = false;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : m.devices_silence_err_lift();
		} finally {
			busy = false;
		}
	}

	/** One look for the entries of the monitor menu (inline): icon, label, 40 px row. */
	const itemClass =
		'flex min-h-10 w-full items-center gap-2 rounded-lg px-2 text-left text-sm font-semibold text-ink hover:bg-surface hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal disabled:opacity-50';

	const triggerLabel = $derived(own ? m.devices_silence_silenced({ until: untilLabel(own) }) : m.devices_silence_menu());
</script>

{#snippet options()}
	<div class={`flex max-w-full flex-col items-stretch ${inline ? 'gap-0.5' : 'w-64 gap-1'}`}>
		{#if own}
			<p class="px-2 py-1 text-sm font-semibold text-advisory-ink">{m.devices_silence_silenced({ until: untilLabel(own) })}</p>
			<Confirm size="md" variant="secondary" class={`w-full justify-start ${inline ? 'min-h-10 gap-2 px-2' : ''}`} confirmLabel={m.devices_silence_lift_confirm()} loading={busy} onconfirm={() => lift(own)}>{#if inline}<BellRing class="size-4 shrink-0" aria-hidden="true" />{/if}{m.devices_silence_lift()}</Confirm>
		{:else}
			{#if global}
				<p class="px-2 py-1 text-sm text-advisory-ink">{m.devices_silence_all({ until: untilLabel(global) })}</p>
			{/if}
			{#if inline}
				<button type="button" role="menuitem" class={itemClass} onclick={silenceOneHour} disabled={busy || picking}>
					<BellOff class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.devices_silence_one_hour()}
				</button>
			{:else}
				<Button variant="ghost" class="w-full justify-start" onclick={silenceOneHour} loading={busy && !picking} disabled={picking}>
					{m.devices_silence_one_hour()}
				</Button>
			{/if}
			{#if !picking}
				{#if inline}
					<button type="button" role="menuitem" class={itemClass} onclick={openPicker} disabled={busy}>
						<CalendarClock class="size-4 shrink-0 text-ink-2" aria-hidden="true" />{m.devices_silence_pick_until()}
					</button>
				{:else}
					<Button variant="ghost" class="w-full justify-start" onclick={openPicker} disabled={busy}>{m.devices_silence_pick_until()}</Button>
				{/if}
			{:else}
				<form class="grid gap-2 px-1 pb-1" onsubmit={submitPicker} aria-label={m.devices_silence_until_label()}>
					<label for="silence-until" class="text-sm font-semibold text-ink">{m.devices_silence_until_label()}</label>
					<DateTimeField id="silence-until" bind:value={until} label={m.devices_silence_until_label()} class="w-[17rem] max-w-full" />
					<div class="flex flex-wrap gap-2">
						<Button type="submit" variant="secondary" loading={busy}>{m.devices_silence_submit()}</Button>
						<Button type="button" variant="ghost" disabled={busy} onclick={() => (picking = false)}>{m.devices_cancel()}</Button>
					</div>
				</form>
			{/if}
		{/if}
		{#if error}
			<p class="px-2 text-[0.8125rem] font-medium text-warning-ink" role="alert">{error}</p>
		{/if}
	</div>
{/snippet}

{#if inline}
	{@render options()}
{:else}
	<Menu label={m.devices_silence_menu()} align="right" bind:open={menuOpen}>
		{#snippet trigger({ toggle, open })}
			<button
				type="button"
				class={`flex size-10 items-center justify-center rounded-lg border hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal ${
					own || global ? 'border-advisory/50 bg-advisory-soft text-advisory-ink' : 'border-line text-ink-2 hover:bg-surface-2'
				}`}
				onclick={toggle}
				aria-haspopup="menu"
				aria-expanded={open}
				aria-label={triggerLabel}
				title={triggerLabel}
			>
				<BellOff class="size-4" aria-hidden="true" />
			</button>
		{/snippet}
		{#snippet children()}
			{@render options()}
		{/snippet}
	</Menu>
{/if}
