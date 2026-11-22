<script lang="ts">
	/**
	 * The monitor's own notification channels: an alert its rule sends to every
	 * channel goes to the ticked ones only (a rule that names its channels keeps
	 * them). Nothing ticked: every channel, as before. Hidden while there is no
	 * channel, or when the list cannot be read (a team account): the monitor then
	 * keeps what it has, since the form only sends a list the person changed.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { listChannels, type Channel } from '#lib/api/index.js';

	interface Props {
		/** Bindable: the ticked channel ids. */
		value: number[];
	}

	let { value = $bindable() }: Props = $props();

	let channels = $state<Channel[]>([]);
	$effect(() => {
		const controller = new AbortController();
		listChannels(controller.signal)
			.then((list) => (channels = list))
			.catch(() => (channels = []));
		return () => controller.abort();
	});
	/** A ticked channel deleted since stays listed, so it can be unticked. */
	const unknown = $derived(value.filter((id) => !channels.some((c) => c.id === id)));

	function toggle(id: number, on: boolean) {
		value = on ? [...value.filter((x) => x !== id), id] : value.filter((x) => x !== id);
	}
</script>

{#if channels.length > 0}
	<fieldset class="grid gap-1">
		<legend class="mb-1 text-sm font-semibold text-ink">{m.w5monitors_form_channels_label()}</legend>
		<p class="mb-1 text-[0.8125rem] text-ink-2">
			{value.length === 0 ? m.w5monitors_form_channels_help_all() : m.w5monitors_form_channels_help_some()}
		</p>
		<div class="grid gap-0.5 sm:grid-cols-2">
			{#each channels as channel (channel.id)}
				<label class="flex min-h-10 min-w-0 items-center gap-3 text-sm text-ink">
					<input
						type="checkbox"
						class="size-4 shrink-0 accent-[var(--color-signal)]"
						checked={value.includes(channel.id)}
						onchange={(e) => toggle(channel.id, e.currentTarget.checked)}
					/>
					<span class="min-w-0 break-words">{channel.name}</span>
					{#if !channel.enabled}
						<span class="text-[0.8125rem] text-ink-3">{m.w5monitors_bulkedit_channel_off()}</span>
					{/if}
				</label>
			{/each}
			{#each unknown as id (id)}
				<label class="flex min-h-10 items-center gap-3 text-sm text-ink">
					<input type="checkbox" class="size-4 shrink-0 accent-[var(--color-signal)]" checked onchange={(e) => toggle(id, e.currentTarget.checked)} />
					{m.w5monitors_form_channel_n({ id })}
				</label>
			{/each}
		</div>
	</fieldset>
{/if}
