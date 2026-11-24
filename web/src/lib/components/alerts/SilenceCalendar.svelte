<script lang="ts">
	/**
	 * Maintenance calendar: one-off and recurring windows unrolled into dated
	 * occurrences (the server expands the weekly and monthly rules), shown as a
	 * month grid or a week. Each occurrence is named and marked by state (active,
	 * upcoming, recurring, past) with an icon and a word, never by colour alone.
	 * Click an occurrence or a busy day to list its windows with their actions
	 * below; click an empty day to schedule a window there. The day buttons form
	 * one tab stop (arrows move by a day or a week, Page Up/Down by a month).
	 * On phones the month cells show a count and the week is a list of days.
	 */
	import { tick } from 'svelte';
	import { CalendarClock, ChevronLeft, ChevronRight, History, Play, Repeat } from 'lucide-svelte';
	import type { Silence } from '#lib/api/index.js';
	import { listSilenceOccurrences } from '#lib/api/index.js';
	import { Button, Plate } from '#lib/ui/index.js';
	import type { Tone } from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { displayPrefs } from '#lib/stores/prefs.svelte.js';
	import { firstDayOfWeek } from '#lib/ui/calendar.js';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import SilenceActions from './SilenceActions.svelte';
	import {
		type CalendarView,
		type ChipKind,
		type DaySegment,
		type Occurrence,
		addDays,
		chipKind,
		dayStart,
		localDay,
		moveFocus,
		queryWindow,
		segmentsByDay,
		shiftAnchor,
		viewDays
	} from './calendar-layout';

	interface Props {
		silences: Silence[];
		view: CalendarView;
		removingId?: number | null;
		busyId?: number | null;
		onremove: (id: number) => void;
		onend?: (silence: Silence) => void;
		onextend?: (silence: Silence, minutes: number) => void;
		onedit?: (silence: Silence) => void;
		onduplicate?: (silence: Silence) => void;
		oncreateday?: (day: string) => void;
	}

	let { silences, view, removingId = null, busyId = null, onremove, onend, onextend, onedit, onduplicate, oncreateday }: Props = $props();

	const locale = getLocale();
	const firstDay = displayPrefs.firstDay ?? firstDayOfWeek(locale);
	const today = localDay(new Date());

	let anchor = $state(today);
	let focused = $state(today);
	let selectedDay = $state<string | null>(null);
	let selectedId = $state<number | null>(null);
	let occurrences = $state<Occurrence[]>([]);
	let loading = $state(true);
	let failed = $state(false);
	let root = $state<HTMLElement | null>(null);

	const days = $derived(viewDays(view, anchor, firstDay));
	const byDay = $derived(segmentsByDay(occurrences, days));
	const weeks = $derived(Array.from({ length: Math.ceil(days.length / 7) }, (_, i) => days.slice(i * 7, i * 7 + 7)));
	const monthKey = $derived(anchor.slice(0, 7));
	const silenceById = $derived(new Map(silences.map((s) => [s.id, s])));

	// Reload when the visible range moves or the windows change (create, edit, end…).
	$effect(() => {
		const { from, to } = queryWindow(days);
		void silences;
		const controller = new AbortController();
		loading = true;
		listSilenceOccurrences(from, to, controller.signal)
			.then((found) => {
				occurrences = found;
				failed = false;
			})
			.catch(() => {
				if (controller.signal.aborted) return;
				failed = true;
				occurrences = [];
			})
			.finally(() => {
				if (!controller.signal.aborted) loading = false;
			});
		return () => controller.abort();
	});

	function format(options: Intl.DateTimeFormatOptions, date: Date): string {
		try {
			return new Intl.DateTimeFormat(locale, options).format(date);
		} catch {
			return localDay(date);
		}
	}
	const dateOf = (iso: string) => dayStart(iso);
	const title = $derived(
		view === 'month'
			? format({ month: 'long', year: 'numeric' }, dateOf(`${monthKey}-01`))
			: `${format({ day: 'numeric', month: 'short' }, dateOf(days[0]))} – ${format({ day: 'numeric', month: 'short', year: 'numeric' }, dateOf(days[6]))}`
	);
	const weekdayShort = (iso: string) => format({ weekday: 'short' }, dateOf(iso));
	const fullDate = (iso: string) => format({ dateStyle: 'full' }, dateOf(iso));
	const timeOf = (minute: number) => format({ hour: 'numeric', minute: '2-digit' }, new Date(2000, 0, 1, 0, minute));
	const range = (segment: DaySegment) => m.w5calendar_chip_time({ start: timeOf(segment.startMinute), end: timeOf(segment.endMinute) });

	const KIND: Record<ChipKind, { tone: Tone; icon: typeof Play; classes: string }> = {
		active: { tone: 'signal', icon: Play, classes: 'border-signal/40 bg-signal-soft text-signal-ink' },
		upcoming: { tone: 'info', icon: CalendarClock, classes: 'border-info/30 bg-info-soft text-info-ink' },
		recurring: { tone: 'advisory', icon: Repeat, classes: 'border-advisory/35 bg-advisory-soft text-advisory-ink' },
		past: { tone: 'muted', icon: History, classes: 'border-line bg-surface-2 text-ink-2' }
	};
	const kindLabel = (kind: ChipKind) =>
		kind === 'active' ? m.w5calendar_kind_active() : kind === 'upcoming' ? m.w5calendar_kind_upcoming() : kind === 'recurring' ? m.w5calendar_kind_recurring() : m.w5calendar_kind_past();
	const LEGEND: ChipKind[] = ['active', 'upcoming', 'recurring', 'past'];

	function shift(direction: -1 | 1) {
		anchor = shiftAnchor(view, anchor, direction);
		focused = anchor;
	}
	function goToday() {
		anchor = today;
		focused = today;
		selectedDay = null;
	}

	async function focusDay(iso: string) {
		await tick();
		root?.querySelector<HTMLButtonElement>(`button[data-day="${iso}"]`)?.focus();
	}

	function onkeydown(event: KeyboardEvent) {
		const next = moveFocus(event.key, focused);
		if (next === null) return;
		event.preventDefault();
		focused = next;
		if (!days.includes(next)) anchor = next;
		void focusDay(next);
	}

	function pickDay(day: string) {
		focused = day;
		const count = byDay.get(day)?.length ?? 0;
		selectedId = null;
		if (count === 0 && auth.canSilence && oncreateday) {
			selectedDay = null;
			oncreateday(day);
			return;
		}
		selectedDay = day;
	}

	function pickOccurrence(segment: DaySegment) {
		focused = segment.day;
		selectedDay = segment.day;
		selectedId = segment.occurrence.silence_id;
	}

	const dayLabel = (day: string) => {
		const count = byDay.get(day)?.length ?? 0;
		return count === 0 ? m.w5calendar_day_label_empty({ date: fullDate(day) }) : m.w5calendar_day_label_busy({ date: fullDate(day), count });
	};

	const agenda = $derived(selectedDay ? (byDay.get(selectedDay) ?? []) : []);
</script>

{#snippet chip(segment: DaySegment, compact: boolean)}
	{@const kind = chipKind(segment.occurrence)}
	{@const meta = KIND[kind]}
	{@const Icon = meta.icon}
	<button
		type="button"
		class={`flex w-full min-w-0 items-center gap-1 rounded-md border px-1.5 py-0.5 text-left text-[0.75rem] leading-tight focus-visible:outline-2 focus-visible:outline-signal ${compact ? '' : 'max-sm:min-h-10'} ${meta.classes} ${selectedId === segment.occurrence.silence_id && selectedDay === segment.day ? 'ring-2 ring-signal' : ''} ${segment.occurrence.enabled ? '' : 'opacity-60'}`}
		onclick={() => pickOccurrence(segment)}
	>
		<Icon class="size-3 shrink-0" aria-hidden="true" />
		<span class="sr-only">{kindLabel(kind)}: </span>
		<span class="truncate font-semibold">{segment.occurrence.name}</span>
		{#if !segment.occurrence.enabled}<span class="sr-only"> {m.w5calendar_disabled()}</span>{/if}
		{#if !compact}<span class="tnum shrink-0 text-ink-2">{timeOf(segment.startMinute)}</span>{/if}
	</button>
{/snippet}

<div bind:this={root} class="grid gap-3" aria-busy={loading}>
	<div class="flex flex-wrap items-center justify-between gap-2">
		<div class="flex items-center gap-1">
			<button type="button" class="inline-flex size-10 items-center justify-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal" aria-label={m.w5calendar_nav_previous()} onclick={() => shift(-1)}>
				<ChevronLeft class="size-4" aria-hidden="true" />
			</button>
			<button type="button" class="inline-flex size-10 items-center justify-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal" aria-label={m.w5calendar_nav_next()} onclick={() => shift(1)}>
				<ChevronRight class="size-4" aria-hidden="true" />
			</button>
			<Button size="sm" variant="ghost" onclick={goToday}>{m.w5calendar_nav_today()}</Button>
		</div>
		<h3 class="text-sm font-semibold text-ink capitalize" aria-live="polite">{title}</h3>
	</div>

	<ul class="flex flex-wrap gap-x-4 gap-y-1 text-[0.75rem] text-ink-2" aria-label={m.w5calendar_legend_label()}>
		{#each LEGEND as kind (kind)}
			{@const Icon = KIND[kind].icon}
			<li class="inline-flex items-center gap-1.5"><Icon class="size-3.5" aria-hidden="true" />{kindLabel(kind)}</li>
		{/each}
	</ul>

	{#if failed}
		<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{m.w5calendar_error_load()}</p>
	{/if}

	{#if view === 'month'}
		<table role="grid" aria-label={`${m.w5calendar_grid_label()}, ${title}`} class="w-full table-fixed border-collapse" {onkeydown}>
			<thead>
				<tr>
					{#each weeks[0] as day (day)}
						<th scope="col" class="h-8 text-center text-[0.75rem] font-medium text-ink-3">{weekdayShort(day)}</th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each weeks as week (week[0])}
					<tr>
						{#each week as day (day)}
							{@const segments = byDay.get(day) ?? []}
							<td class={`h-14 border border-line align-top sm:h-24 ${day.slice(0, 7) === monthKey ? 'bg-surface' : 'bg-canvas-deep'} ${selectedDay === day ? 'outline-2 -outline-offset-2 outline-signal' : ''}`}>
								<button
									type="button"
									data-day={day}
									tabindex={day === focused ? 0 : -1}
									aria-label={dayLabel(day)}
									aria-current={day === today ? 'date' : undefined}
									class={`flex min-h-10 w-full items-center justify-between gap-1 px-1.5 text-left text-sm tnum hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-signal ${day === today ? 'font-bold text-ink underline decoration-2 underline-offset-4' : day.slice(0, 7) === monthKey ? 'text-ink' : 'text-ink-3'}`}
									onclick={() => pickDay(day)}
									onfocus={() => (focused = day)}
								>
									<span>{Number(day.slice(8))}</span>
									{#if segments.length > 0}
										<span class="rounded-full bg-ink-2 px-1.5 text-[0.6875rem] font-semibold text-canvas sm:hidden" aria-hidden="true">{segments.length}</span>
									{/if}
								</button>
								<div class="hidden gap-0.5 px-0.5 pb-0.5 sm:grid">
									{#each segments.slice(0, 2) as segment (segment.key)}
										{@render chip(segment, true)}
									{/each}
									{#if segments.length > 2}
										<button type="button" class="px-1.5 text-left text-[0.75rem] font-medium text-ink-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal" onclick={() => pickDay(day)}>
											{m.w5calendar_more({ count: segments.length - 2 })}
										</button>
									{/if}
								</div>
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
	{:else}
		<!-- Week: seven columns from the sm breakpoint, a list of days on phones. -->
		<div role="presentation" {onkeydown}>
		<ol class="grid gap-2 sm:grid-cols-7" aria-label={`${m.w5calendar_grid_label()}, ${title}`}>
			{#each days as day (day)}
				{@const segments = byDay.get(day) ?? []}
				<li class={`min-w-0 rounded-lg border border-line bg-surface p-1.5 ${selectedDay === day ? 'outline-2 outline-signal' : ''}`}>
					<button
						type="button"
						data-day={day}
						tabindex={day === focused ? 0 : -1}
						aria-label={dayLabel(day)}
						aria-current={day === today ? 'date' : undefined}
						class={`flex min-h-10 w-full items-center gap-2 rounded-md px-1.5 text-left text-sm hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-signal ${day === today ? 'font-bold text-ink underline decoration-2 underline-offset-4' : 'text-ink-2'}`}
						onclick={() => pickDay(day)}
						onfocus={() => (focused = day)}
					>
						<span>{weekdayShort(day)}</span>
						<span class="tnum">{Number(day.slice(8))}</span>
					</button>
					<div class="mt-1 grid gap-1">
						{#each segments as segment (segment.key)}
							{@render chip(segment, false)}
						{/each}
					</div>
				</li>
			{/each}
		</ol>
		</div>
	{/if}

	{#if selectedDay}
		<section class="rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3 shadow-lift" aria-label={m.w5calendar_agenda_title({ date: fullDate(selectedDay) })}>
			<h4 class="text-sm font-semibold text-ink">{m.w5calendar_agenda_title({ date: fullDate(selectedDay) })}</h4>
			{#if agenda.length === 0}
				<p class="mt-1 text-[0.8125rem] text-ink-2">{m.w5calendar_agenda_empty()}</p>
			{/if}
			<ul class="mt-2 grid gap-3">
				{#each agenda as segment (segment.key)}
					{@const kind = chipKind(segment.occurrence)}
					{@const silence = silenceById.get(segment.occurrence.silence_id)}
					<li class={`flex flex-wrap items-start gap-x-4 gap-y-2 ${selectedId === segment.occurrence.silence_id ? 'rounded-md bg-surface-2 p-2' : ''}`}>
						<div class="min-w-0 flex-1">
							<div class="flex flex-wrap items-center gap-2">
								<Plate tone={KIND[kind].tone} label={kindLabel(kind)} size="sm" />
								<span class="min-w-0 truncate font-semibold text-ink" title={segment.occurrence.name}>{segment.occurrence.name}</span>
								{#if !segment.occurrence.enabled}<Plate tone="ghost" label={m.w5calendar_disabled()} size="sm" />{/if}
							</div>
							<p class="mt-0.5 text-[0.8125rem] text-ink-2 tnum">
								{range(segment)}
								{#if segment.continuesBefore} · {m.w5calendar_continues_before()}{/if}
								{#if segment.continuesAfter} · {m.w5calendar_continues_after()}{/if}
							</p>
						</div>
						{#if silence}
							<SilenceActions {silence} {removingId} {busyId} {onremove} {onend} {onextend} {onedit} {onduplicate} />
						{/if}
					</li>
				{/each}
			</ul>
			{#if auth.canSilence && oncreateday}
				<div class="mt-3">
					<Button size="sm" variant="secondary" onclick={() => selectedDay && oncreateday(selectedDay)}>{m.w5calendar_agenda_create()}</Button>
				</div>
			{/if}
		</section>
	{/if}
</div>
