<script lang="ts">
	/**
	 * A day and a time side by side: `DatePicker` and `TimeField`, in place of
	 * `<input type="datetime-local">`. The value keeps that input's format,
	 * `YYYY-MM-DDTHH:mm` in local time (empty while no day is chosen), so the
	 * conversions the forms already do (to UTC, to the API) are unchanged.
	 * `min` and `max` (same format) bound the days; the time is the form's to
	 * check, as it was. A day chosen before any time takes `defaultTime`.
	 */
	import { untrack } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import DatePicker from './DatePicker.svelte';
	import TimeField from './TimeField.svelte';
	import { joinDateTime, splitDateTime } from './calendar.js';

	interface Props {
		/** `YYYY-MM-DDTHH:mm`, or empty. Bindable. */
		value: string;
		/** Id of the day button (what a `Field` label points at); the time gets `<id>-time`. */
		id?: string;
		min?: string | null;
		max?: string | null;
		disabled?: boolean;
		clearable?: boolean;
		invalid?: boolean;
		/** The field's label, to name the time box ("Starts, time"). */
		label?: string;
		defaultTime?: string;
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
		invalid = false,
		label,
		defaultTime = '00:00',
		class: className = '',
		onchange
	}: Props = $props();

	const initial = untrack(() => splitDateTime(value));
	let date = $state(initial.date);
	let time = $state(initial.time);

	// A value set from outside (a default, a reset) splits back into the two boxes.
	$effect(() => {
		const outer = value;
		untrack(() => {
			if (outer === joinDateTime(date, time, defaultTime)) return;
			const parts = splitDateTime(outer);
			date = parts.date;
			time = parts.time;
		});
	});

	function emit() {
		if (date && !time) time = defaultTime;
		const next = joinDateTime(date, time, defaultTime);
		if (next === value) return;
		value = next;
		onchange?.(next);
	}

	const minDay = $derived(min ? min.slice(0, 10) : null);
	const maxDay = $derived(max ? max.slice(0, 10) : null);
	const timeLabel = $derived(label ? m.app_ui_time_of({ label }) : m.app_ui_time_label());
</script>

<div class={`grid min-w-0 grid-cols-[minmax(0,1fr)_minmax(0,6.75rem)] gap-2 ${className}`}>
	<DatePicker bind:value={date} {id} min={minDay} max={maxDay} {disabled} {clearable} {invalid} {label} onchange={emit} />
	<TimeField bind:value={time} id={id ? `${id}-time` : undefined} label={timeLabel} {disabled} {invalid} onchange={emit} />
</div>
