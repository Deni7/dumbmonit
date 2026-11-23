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
	import { EmptyState, Plate, Button } from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { CalendarClock, Search } from 'lucide-svelte';
	import { formatDateTime, parseServerDate } from '#lib/format.js';
	import { scheduleLabel, silenceScope } from './helpers';
	import { m } from '#lib/paraglide/messages.js';
	import FileManagedBadge from '#lib/components/settings/FileManagedBadge.svelte';
	import Segmented from '#lib/components/devices/Segmented.svelte';
	import SilenceActions from './SilenceActions.svelte';
	import SilenceCalendar from './SilenceCalendar.svelte';

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
		/** A click on an empty day of the calendar: schedule a window there (`YYYY-MM-DD`). */
		oncreateday?: (day: string) => void;
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
		onduplicate,
		oncreateday
	}: Props = $props();

	type View = 'list' | 'month' | 'week';
	let view = $state<View>('list');
	const viewOptions = $derived([
		{ id: 'list' as const, label: m.w5calendar_view_list() },
		{ id: 'month' as const, label: m.w5calendar_view_month() },
		{ id: 'week' as const, label: m.w5calendar_view_week() }
	]);

	type Quick = 'all' | 'active' | 'scheduled' | 'recurring';
	let search = $state('');
	let quick = $state<Quick>('all');

	function isRecurring(silence: Silence): boolean {
		return silence.schedule.kind === 'weekly' || silence.schedule.kind === 'monthly';
	}

	/** A one-off window already over (ended with "End now") waits for the server purge: hide it. */
	function isOver(silence: Silence): boolean {
		if (silence.active_now || silence.schedule.kind !== 'once') return false;
		const end = parseServerDate(silence.schedule.ends_at);
		return end !== null && end.getTime() <= Date.now();
	}

	const searched = $derived.by(() => {
		const term = search.trim().toLowerCase();
		const live = silences.filter((s) => !isOver(s));
		if (!term) return live;
		return live.filter(
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
</script>

{#if silences.length === 0}
	<EmptyState
		icon={CalendarClock}
		title={m.alerts_silences_empty_title()}
		description={m.alerts_silences_empty_description()}
	>
		{#snippet action()}
			{#if auth.canSilence}
				<Button variant="primary" onclick={onschedule}>{m.alerts_silences_schedule()}</Button>
			{/if}
		{/snippet}
	</EmptyState>
{:else}
	<div class="mb-3">
		<Segmented
			options={viewOptions}
			value={view}
			onchange={(next) => (view = next)}
			label={m.w5calendar_view_label()}
			size="sm"
		/>
	</div>
	{#if view !== 'list'}
		<SilenceCalendar
			{silences}
			{view}
			{removingId}
			{busyId}
			{onremove}
			{onend}
			{onextend}
			{onedit}
			{onduplicate}
			{oncreateday}
		/>
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
				<SilenceActions {silence} {removingId} {busyId} {onremove} {onend} {onextend} {onedit} {onduplicate} />
			</div>
		{/each}
	</div>
	{/if}
{/if}
