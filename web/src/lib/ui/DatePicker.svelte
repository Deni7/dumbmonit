<script lang="ts">
	/**
	 * A day, chosen on the app's own calendar instead of the device's: the
	 * same box as every `.input`, opening a month grid (`CalendarGrid`) in a
	 * small floating panel. The value is `YYYY-MM-DD` (empty: none), exactly
	 * what `<input type="date">` exchanged, so forms and the API keep their
	 * format. The trigger is a real button (a `Field` label points at it with
	 * `for`): Enter, Space or Arrow Down open the panel, focus lands on the
	 * chosen day (or today), Escape closes it and gives focus back, and leaving
	 * it with Tab closes it too. `clearable` adds a button that empties the field.
	 */
	import { CalendarDays, X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import CalendarGrid from './CalendarGrid.svelte';
	import { clampIso, inRange, parseIsoDate, todayIso, utcNoon } from './calendar.js';
	import { anchorPanel } from './popover.js';

	interface Props {
		/** `YYYY-MM-DD`, or empty. Bindable. */
		value: string;
		id?: string;
		min?: string | null;
		max?: string | null;
		disabled?: boolean;
		/** Shows an "empty" button while a day is set. */
		clearable?: boolean;
		/** Text while empty. */
		placeholder?: string;
		/** Accessible name of the calendar; defaults to "Choose a date". */
		label?: string;
		invalid?: boolean;
		class?: string;
		onchange?: (value: string) => void;
	}

	let {
		value = $bindable(''),
		id,
		min = null,
		max = null,
		disabled = false,
		clearable = false,
		placeholder,
		label,
		invalid = false,
		class: className = '',
		onchange
	}: Props = $props();

	const locale = getLocale();
	const uid = $props.id();
	const panelId = `${uid}-calendar`;

	let open = $state(false);
	let focused = $state(todayIso());
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);

	const shown = $derived.by(() => {
		if (!parseIsoDate(value)) return '';
		try {
			return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: 'UTC' }).format(utcNoon(value));
		} catch {
			return value;
		}
	});

	function show() {
		if (disabled) return;
		focused = clampIso(parseIsoDate(value) ? value : todayIso(), min, max);
		open = true;
	}

	function close(refocus: boolean) {
		open = false;
		if (refocus) trigger?.focus();
	}

	function pick(iso: string) {
		value = iso;
		onchange?.(iso);
		close(true);
	}

	function clear() {
		value = '';
		onchange?.('');
		trigger?.focus();
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
		if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
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

	const todayAllowed = $derived(inRange(todayIso(), min, max));
</script>

<div bind:this={root} class={`relative min-w-0 ${className}`} {onfocusout}>
	<button
		bind:this={trigger}
		{id}
		type="button"
		class={`input flex items-center gap-2 text-left tnum ${clearable && value ? '!pr-11' : ''}`}
		aria-haspopup="dialog"
		aria-expanded={open}
		aria-controls={open ? panelId : undefined}
		aria-invalid={invalid ? 'true' : undefined}
		{disabled}
		onclick={() => (open ? close(false) : show())}
		onkeydown={ontriggerkeydown}
	>
		<CalendarDays class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
		<span class={`min-w-0 flex-1 truncate ${shown ? 'text-ink' : 'text-ink-3'}`}>{shown || placeholder || m.app_ui_date_placeholder()}</span>
	</button>
	{#if clearable && value && !disabled}
		<button
			type="button"
			class="absolute inset-y-0 right-0 inline-flex w-10 items-center justify-center rounded-r-lg text-ink-3 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
			aria-label={m.app_ui_date_clear()}
			onclick={clear}
		>
			<X class="size-4" aria-hidden="true" />
		</button>
	{/if}
	{#if open}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			bind:this={panel}
			id={panelId}
			popover="manual"
			role="dialog"
			aria-label={label ?? m.app_ui_date_choose()}
			class="motion-pop fixed inset-auto z-40 m-0 w-[min(calc(100vw-1rem),20.5rem)] overflow-y-auto rounded-[var(--radius-card)] border border-line bg-surface p-3 text-ink shadow-float"
			onkeydown={onpanelkeydown}
		>
			<CalendarGrid bind:focused selected={value || null} {min} {max} label={label ?? m.app_ui_date_choose()} autofocus onpick={pick} />
			{#if todayAllowed}
				<div class="mt-2 flex justify-end border-t border-line pt-2">
					<button
						type="button"
						class="inline-flex h-10 items-center rounded-lg px-3 text-sm font-semibold text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
						onclick={() => pick(todayIso())}
					>
						{m.app_ui_date_today()}
					</button>
				</div>
			{/if}
		</div>
	{/if}
</div>
