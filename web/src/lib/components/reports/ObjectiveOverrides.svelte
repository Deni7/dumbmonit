<script lang="ts">
	/**
	 * Objectives of single groups or monitors, which replace the report's
	 * objective for those rows (a monitor's own beats its group's). One list:
	 * what is set, each with a remove button, and one line to add: a group or a
	 * monitor, a percentage, "Add". The parent owns `objectives` (bindable).
	 */
	import { X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { listTargets, type Target } from '#lib/api/index.js';
	import {
		UNGROUPED_GROUP,
		objectiveRows,
		parseObjective,
		scopeGroup,
		withObjective,
		withoutObjective,
		type ObjectiveKind,
		type ReportObjectives
	} from '#lib/api/reports.js';
	import { Button, Field } from '#lib/ui/index.js';

	interface Props {
		objectives: ReportObjectives;
		/** Unique per form: ids of the controls. */
		idPrefix: string;
		disabled?: boolean;
	}

	let { objectives = $bindable(), idPrefix, disabled = false }: Props = $props();

	let targets = $state<Target[]>([]);
	$effect(() => {
		const controller = new AbortController();
		listTargets(controller.signal)
			.then((list) => (targets = Array.isArray(list) ? list : []))
			.catch(() => (targets = []));
		return () => controller.abort();
	});

	const groupLabel = (name: string) => (name === UNGROUPED_GROUP ? m.reports_layout_scope_ungrouped() : name);
	const groupNames = $derived([...new Set(targets.map((target) => scopeGroup(target.group_name)))].sort((a, b) => (a === UNGROUPED_GROUP ? 1 : b === UNGROUPED_GROUP ? -1 : a.localeCompare(b))));
	const monitorNames = $derived([...new Set(targets.map((target) => target.name))].sort((a, b) => a.localeCompare(b)));

	const rows = $derived(objectiveRows(objectives));

	let picked = $state('');
	let value = $state('');
	let error = $state<string | null>(null);

	function add() {
		const [kind, ...rest] = picked.split(':');
		const name = rest.join(':');
		if (!name || (kind !== 'g' && kind !== 'm')) {
			error = m.reports_objectives_pick_required();
			return;
		}
		const goal = parseObjective(value);
		if (goal === null) {
			error = m.settings_report_objective_invalid();
			return;
		}
		objectives = withObjective(objectives, kind === 'g' ? 'group' : 'monitor', name, goal);
		picked = '';
		value = '';
		error = null;
	}

	function remove(kind: ObjectiveKind, name: string) {
		objectives = withoutObjective(objectives, kind, name);
	}
</script>

<fieldset class="grid min-w-0 gap-3" {disabled}>
	<legend class="mb-0.5 block text-sm font-semibold text-ink">{m.reports_objectives_title()}</legend>
	<p class="text-[0.8125rem] text-ink-2">{m.reports_objectives_help()}</p>

	{#if rows.length > 0}
		<ul class="divide-y divide-line rounded-lg border border-line" role="list">
			{#each rows as row (row.kind + row.name)}
				<li class="flex min-h-10 items-center gap-3 px-3 py-1">
					<span class="min-w-0 flex-1 text-sm text-ink">
						<span class="break-words">{row.kind === 'group' ? groupLabel(row.name) : row.name}</span>
						<span class="ml-1 text-[0.8125rem] text-ink-2">{row.kind === 'group' ? m.reports_objectives_kind_group() : m.reports_objectives_kind_monitor()}</span>
					</span>
					<span class="tnum text-sm font-semibold text-ink">{row.value}%</span>
					<button
						type="button"
						class="inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
						aria-label={m.reports_objectives_remove({ name: row.kind === 'group' ? groupLabel(row.name) : row.name })}
						onclick={() => remove(row.kind, row.name)}
					>
						<X class="size-4" aria-hidden="true" />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_8rem_auto] sm:items-end">
		<Field label={m.reports_objectives_for()} for={`${idPrefix}-objective-for`}>
			<select id={`${idPrefix}-objective-for`} class="input" bind:value={picked} onchange={() => (error = null)}>
				<option value="">{m.reports_objectives_choose()}</option>
				{#if groupNames.length > 0}
					<optgroup label={m.reports_objectives_groups()}>
						{#each groupNames as name (name)}<option value={`g:${name}`}>{groupLabel(name)}</option>{/each}
					</optgroup>
				{/if}
				{#if monitorNames.length > 0}
					<optgroup label={m.reports_objectives_monitors()}>
						{#each monitorNames as name (name)}<option value={`m:${name}`}>{name}</option>{/each}
					</optgroup>
				{/if}
			</select>
		</Field>
		<Field label={m.settings_report_objective()} for={`${idPrefix}-objective-value`}>
			<input
				id={`${idPrefix}-objective-value`}
				type="text"
				inputmode="decimal"
				class="input"
				bind:value
				oninput={() => (error = null)}
				onkeydown={(event) => {
					if (event.key === 'Enter') {
						event.preventDefault();
						add();
					}
				}}
				placeholder="99.9"
				autocomplete="off"
			/>
		</Field>
		<Button variant="secondary" onclick={add}>{m.reports_objectives_add()}</Button>
	</div>
	{#if error}
		<p class="text-[0.8125rem] text-warning" role="alert">{error}</p>
	{/if}
</fieldset>
