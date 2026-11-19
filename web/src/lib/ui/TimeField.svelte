<script lang="ts">
	/**
	 * A time of day in the same box as every `.input`, instead of the device's
	 * time wheel. The value is `HH:mm` on 24 hours (empty: none), what
	 * `<input type="time">` exchanged. Typing is forgiving (`9`, `930`, `9h30`,
	 * `2:30 pm`) and is tidied to `HH:mm` when the field is left; until then
	 * the value only changes when the text reads as a time. Arrow Up/Down move
	 * by `step` minutes, Page Up/Down by an hour. Text that is not a time marks
	 * the field invalid instead of being thrown away.
	 */
	import { untrack } from 'svelte';
	import { Clock } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { parseTime, shiftTime } from './calendar.js';

	interface Props {
		/** `HH:mm`, or empty. Bindable. */
		value: string;
		id?: string;
		disabled?: boolean;
		/** Minutes moved by Arrow Up/Down. */
		step?: number;
		/** Accessible name when no `<label for>` points here. */
		label?: string;
		invalid?: boolean;
		class?: string;
		onchange?: (value: string) => void;
	}

	let { value = $bindable(''), id, disabled = false, step = 15, label, invalid = false, class: className = '', onchange }: Props = $props();

	let text = $state(untrack(() => value));
	let unreadable = $state(false);

	// A value set from outside (a preset, a reset) shows as typed text; what
	// is being typed is left alone while it still reads as the same value.
	$effect(() => {
		const outer = value;
		untrack(() => {
			if (parseTime(text) !== (outer || null)) {
				text = outer;
				unreadable = false;
			}
		});
	});

	function set(next: string) {
		if (next === value) return;
		value = next;
		onchange?.(next);
	}

	function oninput(event: Event & { currentTarget: HTMLInputElement }) {
		text = event.currentTarget.value;
		const parsed = parseTime(text);
		unreadable = text.trim() !== '' && parsed === null;
		if (parsed !== null) set(parsed);
		else if (text.trim() === '') set('');
	}

	function onblur() {
		const parsed = parseTime(text);
		if (parsed !== null) text = parsed;
	}

	function onkeydown(event: KeyboardEvent) {
		const deltas: Record<string, number | undefined> = { ArrowUp: step, ArrowDown: -step, PageUp: 60, PageDown: -60 };
		const delta = deltas[event.key];
		if (delta === undefined) return;
		event.preventDefault();
		const next = shiftTime(parseTime(text) ?? (value || '00:00'), delta);
		text = next;
		unreadable = false;
		set(next);
	}
</script>

<div class={`relative min-w-0 ${className}`}>
	<Clock class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
	<input
		{id}
		type="text"
		inputmode="numeric"
		autocomplete="off"
		spellcheck="false"
		class="input tnum !pl-9"
		placeholder={m.app_ui_time_placeholder()}
		aria-label={label}
		aria-invalid={invalid || unreadable ? 'true' : undefined}
		maxlength="8"
		{disabled}
		value={text}
		{oninput}
		{onblur}
		{onkeydown}
	/>
</div>
