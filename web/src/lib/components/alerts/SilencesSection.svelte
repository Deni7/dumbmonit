<script lang="ts">
	/**
	 * Scheduled maintenance: the silences, each as a row. A window covering now
	 * is "Active now"; otherwise it is "Scheduled". Removing one is a two-step
	 * confirm — a silence that vanishes by accident lets a real alert through.
	 *
	 * A one-off window that is running can be ended now or extended; any window
	 * (except "until recovery") can be edited or duplicated. Ending is not
	 * removing: the window stays in the list until it expires.
	 */
	import type { Silence, Target } from '#lib/api/index.js';
	import { Confirm, EmptyState, Plate, Button, Menu } from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { CalendarClock, Search } from 'lucide-svelte';
	import { formatDateTime } from '#lib/format.js';
	import { scheduleLabel, silenceScope } from './helpers';
	import { m } from '#lib/paraglide/messages.js';
	import FileManagedBadge from '#lib/components/settings/FileManagedBadge.svelte';
	import Segmented from '#lib/components/devices/Segmented.svelte';

	interface Props {
		silences: Silence[];
		targets: Map<number, Target>;
		removingId?: number | null;
		/** Id of the window being ended or extended. */
		busyId?: number | null;
		onremove: (id: number) => void;
		onschedule: () => void;
		onend?: (silence: Silence) => void;
		onextend?: (silence: Silence, minutes: number) => void;
		onedit?: (silence: Silence) => void;
		onduplicate?: (silence: Silence) => void;
	}

	let {
		silences,
		targets,
		removingId = null,
		busyId = null,
		onremove,
		onschedule,
		onend,
		onextend,
		onedit,
		onduplicate
	}: Props = $props();

	type Quick = 'all' | 'active' | 'scheduled' | 'recurring';
	let search = $state('');
	let quick = $state<Quick>('all');

	function isRecurring(silence: Silence): boolean {
		return silence.schedule.kind === 'weekly' || silence.schedule.kind === 'monthly';
	}

	const searched = $derived.by(() => {
		const term = search.trim().toLowerCase();
		if (!term) return silences;
		return silences.filter(
			(s) => s.name.toLowerCase().includes(term) || s.comment.toLowerCase().includes(term)
		);
	});
	const quickOptions = $derived([
		{ id: 'all' as const, label: m.w1alerts_silences_filter_all() },
		{ id: 'active' as const, label: m.alerts_silences_active_now() },
		{ id: 'scheduled' as const, label: m.alerts_silences_scheduled() },
		{ id: 'recurring' as const, label: m.w1alerts_silences_filter_recurring() }
	]);
	const shown = $derived(
		searched.filter((s) => {
			if (quick === 'active') return s.active_now;
			if (quick === 'scheduled') return !s.active_now;
			if (quick === 'recurring') return isRecurring(s);
			return true;
		})
	);

	/** Only a one-off window has an end to move. */
	function isOneOff(silence: Silence): boolean {
		return silence.schedule.kind === 'once';
	}

	const EXTENSIONS = $derived([
		{ minutes: 30, label: m.w1alerts_silences_extend_30() },
		{ minutes: 60, label: m.w1alerts_silences_extend_60() },
		{ minutes: 240, label: m.w1alerts_silences_extend_240() }
	]);
</script>

{#if silences.length === 0}
	<EmptyState
		icon={CalendarClock}
		title={m.alerts_silences_empty_title()}
		description={m.alerts_silences_empty_description()}
	>
		{#snippet action()}
			{#if auth.canOperate}
				<Button variant="primary" onclick={onschedule}>{m.alerts_silences_schedule()}</Button>
			{/if}
		{/snippet}
	</EmptyState>
{:else}
	{#if silences.length > 3}
		<div class="mb-3 flex flex-col gap-3 sm:flex-row sm:items-center">
			<label class="relative min-w-0 flex-1 sm:max-w-sm">
				<span class="sr-only">{m.w1alerts_silences_search_label()}</span>
				<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
				<input
					type="search"
					class="input !pl-9"
					placeholder={m.w1alerts_silences_search_placeholder()}
					bind:value={search}
					autocomplete="off"
				/>
			</label>
			<Segmented
				options={quickOptions}
				value={quick}
				onchange={(next) => (quick = next)}
				label={m.w1alerts_silences_filter_label()}
				size="sm"
				class="max-sm:w-full"
			/>
		</div>
	{/if}
	{#if shown.length === 0}
		<p class="py-3 text-[0.8125rem] text-ink-2" aria-live="polite">{m.w1alerts_silences_nomatch()}</p>
	{/if}
	<div class="space-y-2.5">
		{#each shown as silence, i (silence.id)}
			<div
				class="rise-in flex flex-wrap items-start gap-x-4 gap-y-2 rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3 shadow-lift"
				style={`--rise-delay: ${i * 30}ms`}
			>
				<div class="min-w-0 flex-1">
					<div class="flex flex-wrap items-center gap-2">
						{#if silence.active_now}
							<Plate tone="signal" label={m.alerts_silences_active_now()} pulse />
						{:else}
							<Plate tone="ghost" label={m.alerts_silences_scheduled()} />
						{/if}
						<span class="truncate font-semibold text-ink">{silence.name}</span>
						<FileManagedBadge kind="silence" id={silence.name} />
					</div>
					<!-- Phones: scope and the one date that matters (until / next), no separators, no comment. -->
					<div class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[0.8125rem] text-ink-2">
						<span>{silenceScope(silence, targets)}</span>
						<span class="text-ink-3 max-sm:hidden" aria-hidden="true">·</span>
						<span class={`tnum ${silence.active_now && silence.active_until ? 'max-sm:hidden' : ''}`}>{scheduleLabel(silence.schedule)}</span>
						{#if silence.active_now && silence.active_until}
							<span class="text-ink-3 max-sm:hidden" aria-hidden="true">·</span>
							<span class="tnum">{m.alerts_silences_until({ when: formatDateTime(silence.active_until) })}</span>
						{:else if silence.next_start_at}
							<span class="text-ink-3 max-sm:hidden" aria-hidden="true">·</span>
							<span class="tnum">{m.alerts_silences_next({ when: formatDateTime(silence.next_start_at) })}</span>
						{/if}
					</div>
					{#if silence.comment}
						<p class="mt-1 text-[0.8125rem] text-ink-2 max-sm:hidden">{silence.comment}</p>
					{/if}
				</div>
				{#if auth.canOperate}
					<div class="flex flex-wrap items-center gap-1.5">
						{#if silence.active_now && isOneOff(silence)}
							<Button
								size="sm"
								variant="secondary"
								loading={busyId === silence.id}
								disabled={busyId !== null}
								onclick={() => onend?.(silence)}
							>
								{m.w1alerts_silences_end_now()}
							</Button>
							<Menu label={m.w1alerts_silences_extend()} align="right">
								{#snippet trigger({ toggle, open })}
									<Button
										size="sm"
										variant="ghost"
										aria-haspopup="menu"
										aria-expanded={open}
										disabled={busyId !== null}
										onclick={toggle}
									>
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
			</div>
		{/each}
	</div>
{/if}
