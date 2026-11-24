<script lang="ts">
	/** Name and address of the device form; each shows its error once touched or after a submit attempt. */
	import { m } from '#lib/paraglide/messages.js';
	import type { CollectorInfo } from '#lib/api/index.js';
	import { Field } from '#lib/ui/index.js';
	import { suggestName } from './kinds';

	interface Props {
		collector: CollectorInfo;
		name: string;
		address: string;
		nameError: string | null;
		addressError: string | null;
		addressHelp: string | undefined;
		/** The error to show for a field: `undefined` until it was touched or a submit was attempted. */
		shown: (field: string, error: string | null | undefined) => string | undefined;
		touch: (field: string) => void;
	}

	let { collector, name = $bindable(), address = $bindable(), nameError, addressError, addressHelp, shown, touch }: Props = $props();
</script>

<div class="grid gap-4 sm:grid-cols-2">
	<Field label={m.deviceform_form_name_label()} for="target-name" required error={shown('name', nameError)} help={m.deviceform_form_name_help()}>
		<input
			id="target-name"
			class="input"
			type="text"
			autocomplete="off"
			bind:value={name}
			placeholder={suggestName(address) || m.deviceform_form_name_placeholder()}
			aria-invalid={shown('name', nameError) ? 'true' : undefined}
			onblur={() => touch('name')}
		/>
	</Field>

	<Field label={m.deviceform_form_address_label()} for="target-address" required error={shown('address', addressError)} help={addressHelp}>
		<div class="relative">
			<input
				id="target-address"
				class={`input font-mono text-[0.8125rem] ${collector.default_port > 0 ? 'pr-16' : ''}`}
				type="text"
				autocomplete="off"
				spellcheck="false"
				bind:value={address}
				placeholder={collector.address_hint || m.deviceform_form_address_placeholder()}
				aria-invalid={shown('address', addressError) ? 'true' : undefined}
				onblur={() => {
					touch('address');
					// Naming a switch is not a prerequisite to watching it.
					if (!name.trim()) name = suggestName(address);
				}}
			/>
			{#if collector.default_port > 0 && !/:\d+$/.test(address.trim())}
				<span
					class="tnum pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 font-mono text-[0.8125rem] text-ink-3"
					aria-hidden="true"
				>
					:{collector.default_port}
				</span>
			{/if}
		</div>
	</Field>
</div>
