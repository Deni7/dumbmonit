<script lang="ts">
	/**
	 * Actions of one maintenance window: end now and extend (a running one-off
	 * window), edit, duplicate, remove (two-step confirm). Shared by the list
	 * and the calendar's day agenda.
	 */
	import type { Silence } from '#lib/api/index.js';
	import { Confirm, Button, Menu } from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		silence: Silence;
		removingId?: number | null;
		busyId?: number | null;
		onremove: (id: number) => void;
		onend?: (silence: Silence) => void;
		onextend?: (silence: Silence, minutes: number) => void;
		onedit?: (silence: Silence) => void;
		onduplicate?: (silence: Silence) => void;
	}

	let { silence, removingId = null, busyId = null, onremove, onend, onextend, onedit, onduplicate }: Props = $props();

	const EXTENSIONS = $derived([
		{ minutes: 30, label: m.w1alerts_silences_extend_30() },
		{ minutes: 60, label: m.w1alerts_silences_extend_60() },
		{ minutes: 240, label: m.w1alerts_silences_extend_240() }
	]);
</script>

{#if auth.canOperate}
	<div class="flex flex-wrap items-center gap-1.5">
		{#if silence.active_now && silence.schedule.kind === 'once'}
			<Button size="sm" variant="secondary" loading={busyId === silence.id} disabled={busyId !== null} onclick={() => onend?.(silence)}>
				{m.w1alerts_silences_end_now()}
			</Button>
			<Menu label={m.w1alerts_silences_extend()} align="right">
				{#snippet trigger({ toggle, open })}
					<Button size="sm" variant="ghost" aria-haspopup="menu" aria-expanded={open} disabled={busyId !== null} onclick={toggle}>
						{m.w1alerts_silences_extend()}
					</Button>
				{/snippet}
				{#snippet children({ close })}
					<div class="grid gap-1">
						{#each EXTENSIONS as option (option.minutes)}
							<Button
								size="sm"
								variant="ghost"
								class="!justify-start"
								onclick={() => {
									close();
									onextend?.(silence, option.minutes);
								}}
							>
								{option.label}
							</Button>
						{/each}
					</div>
				{/snippet}
			</Menu>
		{/if}
		{#if silence.schedule.kind !== 'recovery'}
			<Button size="sm" variant="ghost" onclick={() => onedit?.(silence)}>{m.w1alerts_silences_edit()}</Button>
			<Button size="sm" variant="ghost" onclick={() => onduplicate?.(silence)}>{m.w1alerts_silences_duplicate()}</Button>
		{/if}
		<Confirm
			size="sm"
			variant="danger"
			confirmLabel={m.alerts_silences_remove_confirm()}
			loading={removingId === silence.id}
			onconfirm={() => onremove(silence.id)}
		>
			{m.alerts_silences_remove()}
		</Confirm>
	</div>
{/if}
