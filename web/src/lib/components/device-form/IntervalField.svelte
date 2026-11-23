<script lang="ts">
	/** The "Check every" field of the device form: a preset, or a number and a unit ("Custom"). */
	import { m } from '#lib/paraglide/messages.js';
	import { Field } from '#lib/ui/index.js';
	import { INTERVALS } from './kinds';
	import { INTERVAL_UNITS, splitInterval, type IntervalUnit } from './interval';

	interface Props {
		/** A preset value in seconds, as a string, or `CUSTOM_INTERVAL`. */
		intervalChoice: string;
		customValue: string;
		customUnit: IntervalUnit;
		/** Seconds between two checks; `null` while the custom value is not a number. */
		intervalSecs: number | null;
		intervalError: string | null;
		/** Value of the select that opens the number and unit fields. */
		CUSTOM_INTERVAL: string;
	}

	let {
		intervalChoice = $bindable(),
		customValue = $bindable(),
		customUnit = $bindable(),
		intervalSecs,
		intervalError,
		CUSTOM_INTERVAL
	}: Props = $props();

	const unitLabel = (unit: IntervalUnit) =>
		({ s: m.w1monitors_form_unit_seconds(), m: m.w1monitors_form_unit_minutes(), h: m.w1monitors_form_unit_hours(), d: m.w1monitors_form_unit_days() })[unit];
	function chooseInterval(choice: string) {
		// Opening "Custom" starts from the preset just left, so the number means something.
		if (choice === CUSTOM_INTERVAL && intervalChoice !== CUSTOM_INTERVAL && intervalSecs !== null) {
			const split = splitInterval(intervalSecs);
			customValue = String(split.value);
			customUnit = split.unit;
		}
		intervalChoice = choice;
	}
</script>

<Field label={m.deviceform_form_interval_label()} for="target-interval" help={m.deviceform_form_interval_help()} error={intervalChoice === CUSTOM_INTERVAL ? intervalError : null}>
	<select id="target-interval" class="input tnum" value={intervalChoice} onchange={(e) => chooseInterval(e.currentTarget.value)}>
		{#each INTERVALS as option (option.value)}
			<option value={String(option.value)}>{option.label}</option>
		{/each}
		<option value={CUSTOM_INTERVAL}>{m.w1monitors_form_interval_custom()}</option>
	</select>
	{#if intervalChoice === CUSTOM_INTERVAL}
		<div class="mt-2 flex items-center gap-2">
			<input
				class="input tnum !w-28"
				type="number"
				inputmode="numeric"
				min="1"
				step="1"
				bind:value={customValue}
				aria-label={m.w1monitors_form_interval_value_aria()}
				aria-invalid={intervalError ? 'true' : undefined}
			/>
			<select class="input !w-auto" bind:value={customUnit} aria-label={m.w1monitors_form_interval_unit_aria()}>
				{#each INTERVAL_UNITS as unit (unit)}
					<option value={unit}>{unitLabel(unit)}</option>
				{/each}
			</select>
		</div>
	{/if}
</Field>
