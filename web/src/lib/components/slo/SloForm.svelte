<script lang="ts">
	/**
	 * Create or edit an SLO: name, target, rolling window, the monitors it
	 * covers (the SLA report picker: monitors and whole folders) and whether
	 * its burn rate raises alerts. A line under the target says what the
	 * budget allows, in minutes, so "99.9" reads as "43 min a month".
	 */
	import { untrack } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { createSlo, updateSlo, SLO_WINDOWS, type Slo, type SloDetail } from '#lib/api/index.js';
	import { emptyScope, type ReportScope } from '#lib/api/reports.js';
	import { allowedMinutes, parseObjective } from '#lib/slo.js';
	import { Button, ErrorNotice, Field, Panel, Segmented, Toggle } from '#lib/ui/index.js';
	import MonitorPicker from '#lib/components/reports/MonitorPicker.svelte';

	interface Props {
		/** `null`: a new SLO. */
		slo: Slo | null;
		onsaved: (saved: SloDetail) => void;
		oncancel: () => void;
	}

	let { slo, onsaved, oncancel }: Props = $props();

	const initial = untrack(() => slo);
	const prefix = initial ? `slo-${initial.id}` : 'slo-new';

	let name = $state(initial?.name ?? '');
	let description = $state(initial?.description ?? '');
	let objective = $state(initial ? String(initial.objective) : '99.9');
	let windowDays = $state(String(initial?.window_days ?? 30));
	let scope = $state<ReportScope>(initial ? { targets: [...initial.scope.targets], groups: [...initial.scope.groups] } : emptyScope());
	let scopeEmpty = $state(false);
	let enabled = $state(initial?.enabled ?? true);

	let nameError = $state<string | null>(null);
	let saving = $state(false);
	let error = $state<unknown>(null);

	const target = $derived(parseObjective(objective));
	const objectiveError = $derived(objective.trim() !== '' && target === null ? m.slo_form_target_invalid() : null);
	const allows = $derived(
		target === null ? null : m.slo_form_allows({ minutes: Math.round(allowedMinutes(target, Number(windowDays)) * 10) / 10, days: Number(windowDays) })
	);

	const windowOptions = $derived(SLO_WINDOWS.map((days) => ({ value: String(days), label: m.slo_form_window_days({ days }) })));

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		nameError = name.trim() ? null : m.slo_form_name_required();
		if (nameError || target === null || scopeEmpty) return;
		saving = true;
		try {
			const payload = { name: name.trim(), description: description.trim(), objective: target, window_days: Number(windowDays), scope, enabled };
			onsaved(initial ? await updateSlo(initial.id, payload) : await createSlo(payload));
		} catch (cause) {
			error = cause;
		} finally {
			saving = false;
		}
	}
</script>

<Panel title={initial ? m.slo_form_title_edit({ name: initial.name }) : m.slo_form_title_new()}>
	<form class="grid gap-5" onsubmit={submit} novalidate>
		<div class="grid gap-5 lg:grid-cols-2">
			<div class="grid content-start gap-4">
				<Field label={m.slo_form_name()} for={`${prefix}-name`} error={nameError} required>
					<input
						id={`${prefix}-name`}
						type="text"
						class="input"
						bind:value={name}
						oninput={() => (nameError = null)}
						placeholder={m.slo_form_name_placeholder()}
						maxlength="80"
						autocomplete="off"
						disabled={saving}
						aria-invalid={nameError ? 'true' : undefined}
					/>
				</Field>
				<Field label={m.slo_form_description()} for={`${prefix}-description`}>
					<textarea id={`${prefix}-description`} class="input min-h-10" rows="2" bind:value={description} maxlength="500" disabled={saving}></textarea>
				</Field>
				<Field label={m.slo_form_target()} for={`${prefix}-target`} error={objectiveError} help={allows ?? m.slo_form_target_help()} required>
					<input
						id={`${prefix}-target`}
						type="text"
						inputmode="decimal"
						class="input max-w-40 tnum"
						bind:value={objective}
						placeholder="99.9"
						autocomplete="off"
						disabled={saving}
						aria-invalid={objectiveError ? 'true' : undefined}
					/>
				</Field>
				<Segmented legend={m.slo_form_window()} options={windowOptions} bind:value={windowDays} name={`${prefix}-window`} disabled={saving} />
				<Field label={m.slo_form_enabled()} for={`${prefix}-enabled`} help={m.slo_form_enabled_help()} inline>
					<Toggle id={`${prefix}-enabled`} bind:checked={enabled} disabled={saving} label={m.slo_form_enabled()} />
				</Field>
			</div>
			<MonitorPicker bind:scope bind:empty={scopeEmpty} idPrefix={prefix} disabled={saving} />
		</div>

		{#if error}
			<ErrorNotice {error} title={m.slo_form_error()} />
		{/if}
		<div class="flex flex-wrap items-center gap-2 border-t border-line pt-4">
			<Button type="submit" variant="primary" loading={saving} disabled={target === null || scopeEmpty}>
				{initial ? m.slo_form_save() : m.slo_form_create()}
			</Button>
			<Button variant="ghost" onclick={oncancel} disabled={saving}>{m.slo_form_cancel()}</Button>
			{#if scopeEmpty}
				<p class="text-[0.8125rem] text-ink-2">{m.reports_scope_none_selected()}</p>
			{/if}
		</div>
	</form>
</Panel>
