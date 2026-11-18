<script lang="ts">
	/**
	 * One month of days, the body of the date pickers. `focused` is the day
	 * holding the tab stop (its month is the one shown); the parent owns it.
	 * WAI-ARIA date grid: arrows move by a day or a week, Home/End to the start
	 * or end of the week, Page Up/Down by a month (with Shift, a year), Enter or
	 * Space picks. Days outside `min`/`max` stay focusable (the keyboard must be
	 * able to cross them) but cannot be picked. Names of months and weekdays
	 * come from `Intl` in the interface language; the week starts where the
	 * locale says. Selection is shown by fill and by `aria-selected`, today by a
	 * mark under the number and `aria-current`, never by colour alone.
	 */
	import { tick } from 'svelte';
	import { ChevronLeft, ChevronRight } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import {
		addDays,
		addMonths,
		clampIso,
		endOfWeek,
		firstDayOfWeek,
		inRange,
		monthGrid,
		startOfWeek,
		todayIso,
		utcNoon,
		weekdayOrder
	} from './calendar.js';

	interface Props {
		/** Day with the tab stop; bindable. */
		focused: string;
		/** The picked day (single date). */
		selected?: string | null;
		/** A range being shown: both ends included; `preview` stands for a missing end while choosing. */
		rangeStart?: string | null;
		rangeEnd?: string | null;
		preview?: string | null;
		min?: string | null;
		max?: string | null;
		/** Accessible name of the grid. */
		label: string;
		/** Focus the `focused` day once mounted (the popover just opened). */
		autofocus?: boolean;
		onpick: (iso: string) => void;
		/** The pointer or the keyboard is on a day (range preview). */
		onactive?: (iso: string) => void;
	}

	let {
		focused = $bindable(),
		selected = null,
		rangeStart = null,
		rangeEnd = null,
		preview = null,
		min = null,
		max = null,
		label,
		autofocus = false,
		onpick,
		onactive
	}: Props = $props();

	const locale = getLocale();
	const firstDay = firstDayOfWeek(locale);
	const today = todayIso();
	const uid = $props.id();

	const weeks = $derived(monthGrid(focused, firstDay));
	const month = $derived(focused.slice(0, 7));

	const monthTitle = $derived.by(() => {
		try {
			return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(utcNoon(`${month}-01`));
		} catch {
			return month;
		}
	});

	const weekdays = weekdayOrder(firstDay).map((day) => {
		// 2023-01-01 was a Sunday: day `n` of that week has weekday `n`.
		const date = new Date(Date.UTC(2023, 0, 1 + day, 12));
		const format = (weekday: 'short' | 'long') => {
			try {
				return new Intl.DateTimeFormat(locale, { weekday, timeZone: 'UTC' }).format(date);
			} catch {
				return String(day);
			}
		};
		return { short: format('short'), long: format('long') };
	});

	const dayLabel = (iso: string) => {
		try {
			return new Intl.DateTimeFormat(locale, { dateStyle: 'full', timeZone: 'UTC' }).format(utcNoon(iso));
		} catch {
			return iso;
		}
	};

	let grid = $state<HTMLTableElement | null>(null);

	async function focusDay() {
		await tick();
		grid?.querySelector<HTMLButtonElement>(`button[data-day="${focused}"]`)?.focus();
	}

	$effect(() => {
		if (autofocus) void focusDay();
	});

	function go(next: string) {
		focused = next;
		onactive?.(next);
		void focusDay();
	}

	function onkeydown(event: KeyboardEvent) {
		const key = event.key;
		let next: string | null = null;
		if (key === 'ArrowLeft') next = addDays(focused, -1);
		else if (key === 'ArrowRight') next = addDays(focused, 1);
		else if (key === 'ArrowUp') next = addDays(focused, -7);
		else if (key === 'ArrowDown') next = addDays(focused, 7);
		else if (key === 'Home') next = startOfWeek(focused, firstDay);
		else if (key === 'End') next = endOfWeek(focused, firstDay);
		else if (key === 'PageUp') next = addMonths(focused, event.shiftKey ? -12 : -1);
		else if (key === 'PageDown') next = addMonths(focused, event.shiftKey ? 12 : 1);
		if (next !== null) {
			event.preventDefault();
			go(next);
		}
	}

	function pick(iso: string) {
		if (!inRange(iso, min, max)) return;
		onpick(iso);
	}

	/** Month buttons: the shown month moves, the tab stop stays on a reachable day. */
	function shiftMonth(delta: number) {
		focused = clampIso(addMonths(focused, delta), min, max);
	}

	const canPrevious = $derived(!min || `${month}-01` > min);
	const canNext = $derived(!max || `${month}-31` < max);

	const rangeLow = $derived.by(() => {
		const end = rangeEnd ?? preview;
		if (!rangeStart || !end) return null;
		return rangeStart <= end ? rangeStart : end;
	});
	const rangeHigh = $derived.by(() => {
		const end = rangeEnd ?? preview;
		if (!rangeStart || !end) return null;
		return rangeStart <= end ? end : rangeStart;
	});

	function isSelected(iso: string): boolean {
		if (selected) return iso === selected;
		return iso === rangeStart || iso === rangeEnd;
	}
	function isBetween(iso: string): boolean {
		return rangeLow !== null && rangeHigh !== null && iso > rangeLow && iso < rangeHigh;
	}
</script>

<div class="grid gap-2">
	<div class="flex items-center justify-between gap-2">
		<button
			type="button"
			class="inline-flex size-10 items-center justify-center rounded-lg text-ink-2 transition-colors hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal disabled:cursor-not-allowed disabled:opacity-40"
			aria-label={m.app_ui_date_previous_month()}
			disabled={!canPrevious}
			onclick={() => shiftMonth(-1)}
		>
			<ChevronLeft class="size-4" aria-hidden="true" />
		</button>
		<p id={`${uid}-title`} class="text-sm font-semibold text-ink capitalize" aria-live="polite">{monthTitle}</p>
		<button
			type="button"
			class="inline-flex size-10 items-center justify-center rounded-lg text-ink-2 transition-colors hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal disabled:cursor-not-allowed disabled:opacity-40"
			aria-label={m.app_ui_date_next_month()}
			disabled={!canNext}
			onclick={() => shiftMonth(1)}
		>
			<ChevronRight class="size-4" aria-hidden="true" />
		</button>
	</div>

	<table bind:this={grid} role="grid" aria-label={`${label}, ${monthTitle}`} class="w-full border-collapse" {onkeydown}>
		<thead>
			<tr>
				{#each weekdays as name (name.long)}
					<th scope="col" abbr={name.long} class="h-8 text-center text-[0.75rem] font-medium text-ink-3">{name.short}</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each weeks as week (week[0])}
				<tr>
					{#each week as iso (iso)}
						{@const outside = iso.slice(0, 7) !== month}
						{@const allowed = inRange(iso, min, max)}
						{@const chosen = isSelected(iso)}
						{@const between = isBetween(iso)}
						<td role="gridcell" aria-selected={chosen || between} class={`p-0 text-center ${between ? 'bg-signal-soft' : ''} ${chosen && rangeLow && iso === rangeLow && rangeHigh !== rangeLow ? 'bg-gradient-to-r from-transparent from-50% to-signal-soft to-50%' : ''} ${chosen && rangeHigh && iso === rangeHigh && rangeHigh !== rangeLow ? 'bg-gradient-to-l from-transparent from-50% to-signal-soft to-50%' : ''}`}>
							<button
								type="button"
								data-day={iso}
								tabindex={iso === focused ? 0 : -1}
								aria-label={dayLabel(iso)}
								aria-current={iso === today ? 'date' : undefined}
								aria-disabled={allowed ? undefined : 'true'}
								class={`relative mx-auto inline-flex size-10 items-center justify-center rounded-lg text-sm tnum transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-signal ${
									chosen
										? 'bg-primary font-semibold text-on-primary'
										: !allowed
											? 'cursor-not-allowed text-ink-3 line-through decoration-ink-3/60'
											: between
												? 'text-signal-ink hover:bg-surface'
												: outside
													? 'text-ink-3 hover:bg-surface-2 hover:text-ink'
													: 'text-ink hover:bg-surface-2'
								} ${iso === today && !chosen ? 'font-semibold' : ''}`}
								onclick={() => pick(iso)}
								onmouseenter={() => onactive?.(iso)}
								onfocus={() => (focused = iso)}
							>
								{Number(iso.slice(8))}
								{#if iso === today}
									<span class={`absolute bottom-1.5 left-1/2 h-0.5 w-3 -translate-x-1/2 rounded-full ${chosen ? 'bg-on-primary' : 'bg-ink-2'}`} aria-hidden="true"></span>
								{/if}
							</button>
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>
