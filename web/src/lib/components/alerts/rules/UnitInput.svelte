<script lang="ts">
	/**
	 * A duration typed as a number and a unit ("3" "days"): the threshold of a
	 * rule that measures an age or a delay, and how long a condition must last.
	 * The parent keeps the text and the unit and converts on save
	 * (`units.ts`), so an emptied field stays distinct from 0.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import type { DurationUnit } from './units';

	interface Props {
		id: string;
		/** The typed number; a bound number field may hand back a number or null. */
		value: string | number | null;
		unit: DurationUnit;
		choices: DurationUnit[];
		placeholder?: string;
		/** What the number field is, for screen readers ("Threshold"). */
		label: string;
		min?: number;
		disabled?: boolean;
	}

	let { id, value = $bindable(), unit = $bindable(), choices, placeholder, label, min, disabled = false }: Props = $props();

	const unitLabel = (u: DurationUnit) =>
		({
			s: m.w1monitors_form_unit_seconds(),
			m: m.w1monitors_form_unit_minutes(),
			h: m.w1monitors_form_unit_hours(),
			d: m.w1monitors_form_unit_days()
		})[u];
</script>

<div class="flex items-center gap-2">
	<input
		{id}
		type="number"
		step="any"
		{min}
		inputmode="decimal"
		class="input tnum min-w-0 flex-1"
		bind:value
		{placeholder}
		{disabled}
	/>
	<select
		class="input !w-auto shrink-0"
		bind:value={unit}
		{disabled}
		aria-label={m.w8rules_unitinput_aria({ field: label })}
	>
		{#each choices as choice (choice)}
			<option value={choice}>{unitLabel(choice)}</option>
		{/each}
	</select>
</div>
