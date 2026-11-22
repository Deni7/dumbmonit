<script lang="ts">
	/**
	 * A period of whole days on the app's calendar: one box showing the range,
	 * opening a panel with shortcuts (last 7 or 30 days, this month, last month,
	 * last quarter) above a month grid where the first click is the first day
	 * and the second the last (in either order). `from` and `to` are
	 * `YYYY-MM-DD`, both included — what two `<input type="date">` exchanged.
	 * Keyboard as in `DatePicker`; Escape drops a half-chosen range, then
	 * closes. The shortcut matching the range is shown pressed.
	 */
	import { CalendarRange } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import CalendarGrid from './CalendarGrid.svelte';
	import Chip from './Chip.svelte';
	import {
		RANGE_PRESETS,
		clampIso,
		matchingPreset,
		parseIsoDate,
		presetRange,
		todayIso,
		utcNoon,
		type RangePreset
	} from './calendar.js';
	import { anchorPanel } from './popover.js';

	interface Props {
		from: string;
		to: string;
		id?: string;
		min?: string | null;
		max?: string | null;
		disabled?: boolean;
		invalid?: boolean;
		/** Accessible name of the calendar; defaults to "Choose a period". */
		label?: string;
		/** Shortcuts offered above the calendar; all of them by default, `[]` for none. */
		presets?: RangePreset[];
		class?: string;
		onchange?: (range: { from: string; to: string }) => void;
	}

	let {
		from = $bindable(''),
		to = $bindable(''),
		id,
		min = null,
		max = null,
		disabled = false,
		invalid = false,
		label,
		presets = RANGE_PRESETS,
		class: className = '',
		onchange
	}: Props = $props();

	const locale = getLocale();
	const uid = $props.id();
	const panelId = `${uid}-calendar`;

	const PRESET_LABELS: Record<RangePreset, () => string> = {
		last_7_days: () => m.app_ui_range_last_7_days(),
		last_30_days: () => m.app_ui_range_last_30_days(),
		this_month: () => m.app_ui_range_this_month(),
		last_month: () => m.app_ui_range_last_month(),
		last_quarter: () => m.app_ui_range_last_quarter(),
		this_year: () => m.app_ui_range_this_year()
	};

	let open = $state(false);
	let focused = $state(todayIso());
	/** First day clicked, while the second is awaited. */
	let anchor = $state<string | null>(null);
	let hovered = $state<string | null>(null);
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);

	const today = todayIso();
	const active = $derived(from && to ? matchingPreset(from, to, today) : null);
	const offered = $derived(
		presets.filter((preset) => {
			const range = presetRange(preset, today);
			return (!min || range.from >= min) && (!max || range.to <= max);
		})
	);

	const shown = $derived.by(() => {
		if (!parseIsoDate(from) || !parseIsoDate(to)) return '';
		try {
			return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: 'UTC' }).formatRange(utcNoon(from), utcNoon(to));
		} catch {
			return `${from} – ${to}`;
		}
	});

	function show() {
		if (disabled) return;
		anchor = null;
		hovered = null;
		focused = clampIso(parseIsoDate(to) ? to : today, min, max);
		open = true;
	}

	function close(refocus: boolean) {
		open = false;
		anchor = null;
		if (refocus) trigger?.focus();
	}

	function commit(start: string, end: string) {
		const [low, high] = start <= end ? [start, end] : [end, start];
		from = low;
		to = high;
		onchange?.({ from: low, to: high });
		close(true);
	}

	function pick(iso: string) {
		if (anchor === null) {
			anchor = iso;
			hovered = iso;
			return;
		}
		commit(anchor, iso);
	}

	function choosePreset(preset: RangePreset) {
		const range = presetRange(preset, today);
		commit(range.from, range.to);
	}

	$effect(() => {
		if (!open || !panel || !root) return;
		return anchorPanel(root, panel);
	});

	$effect(() => {
		if (!open) return;
		const onpointerdown = (event: PointerEvent) => {
			if (root && !root.contains(event.target as Node)) close(false);
		};
		document.addEventListener('pointerdown', onpointerdown);
		return () => document.removeEventListener('pointerdown', onpointerdown);
	});

	function onpanelkeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		if (anchor !== null) {
			anchor = null;
			hovered = null;
		} else {
			close(true);
		}
	}

	function ontriggerkeydown(event: KeyboardEvent) {
		if (event.key === 'ArrowDown' && !open) {
			event.preventDefault();
			show();
		}
	}

	function onfocusout(event: FocusEvent) {
		const next = event.relatedTarget as Node | null;
		if (open && next && root && !root.contains(next)) close(false);
	}

	const name = $derived(label ?? m.app_ui_range_choose());
</script>

<div bind:this={root} class={`relative min-w-0 ${className}`} {onfocusout}>
	<button
		bind:this={trigger}
		{id}
		type="button"
		class="input flex items-center gap-2 text-left tnum"
		aria-haspopup="dialog"
		aria-expanded={open}
		aria-controls={open ? panelId : undefined}
		aria-invalid={invalid ? 'true' : undefined}
		{disabled}
		onclick={() => (open ? close(false) : show())}
		onkeydown={ontriggerkeydown}
	>
		<CalendarRange class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
		<span class={`min-w-0 flex-1 truncate ${shown ? 'text-ink' : 'text-ink-3'}`}>{shown || m.app_ui_range_placeholder()}</span>
		{#if active}<span class="shrink-0 text-[0.8125rem] text-ink-2 max-sm:hidden">{PRESET_LABELS[active]()}</span>{/if}
	</button>
	{#if open}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			bind:this={panel}
			id={panelId}
			popover="manual"
			role="dialog"
			aria-label={name}
			class="motion-pop fixed inset-auto z-40 m-0 grid w-[min(calc(100vw-1rem),20.5rem)] gap-3 overflow-y-auto rounded-[var(--radius-card)] border border-line bg-surface p-3 text-ink shadow-float"
			onkeydown={onpanelkeydown}
		>
			{#if offered.length > 0}
				<div class="flex flex-wrap gap-1.5" role="group" aria-label={m.app_ui_range_shortcuts()}>
					{#each offered as preset (preset)}
						<Chip pressed={active === preset} onclick={() => choosePreset(preset)}>{PRESET_LABELS[preset]()}</Chip>
					{/each}
				</div>
			{/if}
			<div class="border-t border-line pt-2">
				<CalendarGrid
					bind:focused
					rangeStart={anchor ?? from}
					rangeEnd={anchor === null ? to : null}
					preview={anchor === null ? null : hovered}
					{min}
					{max}
					label={name}
					autofocus
					onpick={pick}
					onactive={(iso) => (hovered = iso)}
				/>
			</div>
			<p class="text-[0.8125rem] text-ink-2" aria-live="polite">{anchor === null ? m.app_ui_range_pick_first() : m.app_ui_range_pick_last()}</p>
		</div>
	{/if}
</div>
