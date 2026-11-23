<script lang="ts">
	/** The "New rule" form of the rules list: a threshold rule with the minimum the server needs. */
	import { m } from '#lib/paraglide/messages.js';
	import type { AlertRulePayload, AlertSeverity, RuleOperator } from '#lib/api/index.js';
	import { Button, Field, Panel } from '#lib/ui/index.js';
	import { severityWord } from '../helpers';

	interface Props {
		oncreate: (payload: AlertRulePayload) => Promise<void>;
		/** Created or cancelled: the list closes the form. */
		ondone: () => void;
	}

	let { oncreate, ondone }: Props = $props();

	const OPERATORS: RuleOperator[] = ['>', '>=', '<', '<='];
	const SEVERITIES: AlertSeverity[] = ['info', 'warning', 'critical'];

	let saving = $state(false);
	let formError = $state<string | null>(null);

	let name = $state('');
	let query = $state('');
	let operator = $state<RuleOperator>('>');
	let threshold = $state('');
	let forSecs = $state('300');
	let severity = $state<AlertSeverity>('warning');

	function reset() {
		name = '';
		query = '';
		operator = '>';
		threshold = '';
		forSecs = '300';
		severity = 'warning';
		formError = null;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		formError = null;
		if (!name.trim()) {
			formError = m.alerts_rules_error_name();
			return;
		}
		if (!query.trim()) {
			formError = m.alerts_rules_error_query();
			return;
		}
		const thresholdValue = Number(threshold);
		if (!Number.isFinite(thresholdValue)) {
			formError = m.alerts_rules_error_threshold();
			return;
		}
		const forValue = Number(forSecs);
		if (!Number.isInteger(forValue) || forValue < 0) {
			formError = m.alerts_rules_error_hold();
			return;
		}
		saving = true;
		try {
			await oncreate({
				name: name.trim(),
				query: query.trim(),
				kind: 'threshold',
				operator,
				threshold: thresholdValue,
				for_secs: forValue,
				severity
			});
			reset();
			ondone();
		} catch (cause) {
			formError = cause instanceof Error ? cause.message : m.alerts_rules_error_create();
		} finally {
			saving = false;
		}
	}
</script>

<Panel title={m.alerts_rules_new()} description={m.alerts_rules_new_description()}>
	<form class="grid gap-4" onsubmit={submit}>
		<Field label={m.alerts_form_name()} for="rule-name" required>
			<input id="rule-name" class="input" bind:value={name} placeholder={m.alerts_rules_name_placeholder()} />
		</Field>
		<Field
			label={m.alerts_rules_query()}
			for="rule-query"
			required
			help={m.alerts_rules_query_help()}
		>
			<input
				id="rule-query"
				class="input font-mono text-[0.8125rem]"
				bind:value={query}
				placeholder="dumbmonit_cpu_usage_percent"
				autocomplete="off"
				spellcheck="false"
			/>
		</Field>
		<div class="grid gap-4 sm:grid-cols-3">
			<Field label={m.alerts_rules_operator()} for="rule-op">
				<select id="rule-op" class="input" bind:value={operator}>
					{#each OPERATORS as op (op)}
						<option value={op}>{op}</option>
					{/each}
				</select>
			</Field>
			<Field label={m.alerts_rules_threshold()} for="rule-threshold" required>
				<input
					id="rule-threshold"
					type="number"
					step="any"
					class="input"
					bind:value={threshold}
					placeholder="90"
				/>
			</Field>
			<Field label={m.alerts_history_severity()} for="rule-severity">
				<select id="rule-severity" class="input" bind:value={severity}>
					{#each SEVERITIES as sev (sev)}
						<option value={sev}>{severityWord(sev)}</option>
					{/each}
				</select>
			</Field>
		</div>
		<Field label={m.alerts_rules_hold_for()} for="rule-for" help={m.alerts_rules_hold_for_help()}>
			<input id="rule-for" type="number" min="0" class="input" bind:value={forSecs} />
		</Field>

		{#if formError}
			<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{formError}</p>
		{/if}

		<div class="flex flex-wrap items-center gap-2">
			<Button type="submit" variant="primary" loading={saving}>{m.alerts_rules_create()}</Button>
			<Button
				type="button"
				variant="ghost"
				disabled={saving}
				onclick={() => {
					ondone();
					reset();
				}}
			>
				{m.alerts_form_cancel()}
			</Button>
		</div>
	</form>
</Panel>
