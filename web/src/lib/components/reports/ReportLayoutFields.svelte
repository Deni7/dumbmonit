<script lang="ts">
	/**
	 * The options of an SLA report, chosen when it is built (on demand or on a
	 * schedule): template, paper, sections, title, notes, logo and the monitors
	 * it covers. The parent owns `layout` and `scope` (bindable); switching the
	 * template resets the sections to that template's starting set, which the
	 * person can then change.
	 */
	import { X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { listTargets, type Target } from '#lib/api/index.js';
	import {
		MAX_REPORT_NOTES,
		MAX_REPORT_TITLE,
		REPORT_SECTION_KEYS,
		UNGROUPED_GROUP,
		sectionsFor,
		type ReportLayout,
		type ReportPaper,
		type ReportScope,
		type ReportSections,
		type ReportTemplate
	} from '#lib/api/reports.js';
	import { Chip, Combobox, Field, Toggle, type ComboOption } from '#lib/ui/index.js';

	interface Props {
		layout: ReportLayout;
		scope: ReportScope;
		/** Unique per form: ids of the controls. */
		idPrefix: string;
		disabled?: boolean;
	}

	let { layout = $bindable(), scope = $bindable(), idPrefix, disabled = false }: Props = $props();

	const TEMPLATES: { value: ReportTemplate; label: string; help: string }[] = [
		{ value: 'detailed', label: m.reports_layout_template_detailed(), help: m.reports_layout_template_detailed_help() },
		{ value: 'summary', label: m.reports_layout_template_summary(), help: m.reports_layout_template_summary_help() }
	];
	const PAPERS: { value: ReportPaper; label: string }[] = [
		{ value: 'a4', label: m.reports_layout_paper_a4() },
		{ value: 'letter', label: m.reports_layout_paper_letter() }
	];
	const SECTION_LABELS: Record<keyof ReportSections, () => string> = {
		summary: () => m.reports_layout_section_summary(),
		chart: () => m.reports_layout_section_chart(),
		monitors: () => (layout.template === 'summary' ? m.reports_layout_section_monitors_summary() : m.reports_layout_section_monitors()),
		incidents: () => (layout.template === 'summary' ? m.reports_layout_section_incidents_summary() : m.reports_layout_section_incidents()),
		maintenance: () => m.reports_layout_section_maintenance(),
		response_times: () => m.reports_layout_section_response_times(),
		notes: () => m.reports_layout_section_notes()
	};

	function pickTemplate(template: ReportTemplate) {
		if (template === layout.template) return;
		layout = { ...layout, template, sections: sectionsFor(template) };
	}

	function setSection(key: keyof ReportSections, on: boolean) {
		layout = { ...layout, sections: { ...layout.sections, [key]: on } };
	}

	// The monitors the person can pick from: those the API shows them.
	let targets = $state<Target[]>([]);
	$effect(() => {
		const controller = new AbortController();
		listTargets(controller.signal)
			.then((list) => (targets = Array.isArray(list) ? list : []))
			.catch(() => (targets = []));
		return () => controller.abort();
	});

	const groupName = (target: Target) => target.group_name.trim() || UNGROUPED_GROUP;
	const groups = $derived([...new Set(targets.map(groupName))].sort((a, b) => a.localeCompare(b)));
	const monitorOptions = $derived<ComboOption[]>(
		targets.map((target) => ({ value: String(target.id), label: target.name, group: target.group_name.trim() || undefined }))
	);
	const byId = $derived(new Map(targets.map((target) => [target.id, target])));
	const scopeEmpty = $derived(scope.targets.length === 0 && scope.groups.length === 0);

	function toggleGroup(name: string) {
		const on = !scope.groups.includes(name);
		scope = { ...scope, groups: on ? [...scope.groups, name] : scope.groups.filter((g) => g !== name) };
	}

	function toggleMonitor(value: string, on: boolean) {
		const id = Number(value);
		const rest = scope.targets.filter((t) => t !== id);
		scope = { ...scope, targets: on ? [...rest, id] : rest };
	}

	const groupLabel = (name: string) => (name === UNGROUPED_GROUP ? m.reports_layout_scope_ungrouped() : name);
</script>

<div class="grid gap-5 sm:grid-cols-2">
	<fieldset class="grid min-w-0 content-start gap-2 sm:col-span-2" {disabled}>
		<legend class="mb-1.5 block text-sm font-semibold text-ink">{m.reports_layout_template()}</legend>
		<div class="grid gap-2 sm:grid-cols-2" role="radiogroup" aria-label={m.reports_layout_template()}>
			{#each TEMPLATES as option (option.value)}
				<label
					class={`flex min-h-11 cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-signal ${layout.template === option.value ? 'border-ink bg-surface' : 'border-line bg-canvas-deep hover:border-line-strong'}`}
				>
					<input
						type="radio"
						class="mt-1 size-4 accent-[var(--c-ink)]"
						name={`${idPrefix}-template`}
						value={option.value}
						checked={layout.template === option.value}
						onchange={() => pickTemplate(option.value)}
					/>
					<span class="grid gap-0.5">
						<span class="text-sm font-semibold text-ink">{option.label}</span>
						<span class="text-[0.8125rem] text-ink-2">{option.help}</span>
					</span>
				</label>
			{/each}
		</div>
	</fieldset>

	<fieldset class="grid min-w-0 content-start gap-1.5 sm:col-span-2" {disabled}>
		<legend class="mb-1.5 block text-sm font-semibold text-ink">{m.reports_layout_sections()}</legend>
		<div class="grid gap-x-4 gap-y-1 sm:grid-cols-2">
			{#each REPORT_SECTION_KEYS as key (key)}
				<label class="flex min-h-10 cursor-pointer items-center gap-2.5 text-sm text-ink">
					<input
						type="checkbox"
						class="size-4 accent-[var(--c-ink)]"
						checked={layout.sections[key]}
						onchange={(event) => setSection(key, event.currentTarget.checked)}
					/>
					{SECTION_LABELS[key]()}
				</label>
			{/each}
		</div>
	</fieldset>

	<Field label={m.reports_layout_title()} for={`${idPrefix}-title`} help={m.reports_layout_title_help()}>
		<input
			id={`${idPrefix}-title`}
			type="text"
			class="input"
			value={layout.title ?? ''}
			oninput={(event) => (layout = { ...layout, title: event.currentTarget.value === '' ? null : event.currentTarget.value })}
			maxlength={MAX_REPORT_TITLE}
			placeholder={m.reports_layout_title_placeholder()}
			autocomplete="off"
			{disabled}
		/>
	</Field>

	<Field label={m.reports_layout_paper()} for={`${idPrefix}-paper`}>
		<select
			id={`${idPrefix}-paper`}
			class="input"
			value={layout.paper}
			onchange={(event) => (layout = { ...layout, paper: event.currentTarget.value === 'letter' ? 'letter' : 'a4' })}
			{disabled}
		>
			{#each PAPERS as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
		</select>
	</Field>

	{#if layout.sections.notes}
		<Field
			label={m.reports_layout_notes()}
			for={`${idPrefix}-notes`}
			help={m.reports_layout_notes_help({ max: MAX_REPORT_NOTES })}
			class="sm:col-span-2"
		>
			<textarea
				id={`${idPrefix}-notes`}
				class="input min-h-24"
				value={layout.notes}
				oninput={(event) => (layout = { ...layout, notes: event.currentTarget.value })}
				maxlength={MAX_REPORT_NOTES}
				{disabled}
			></textarea>
		</Field>
	{/if}

	<Field label={m.reports_layout_logo()} help={m.reports_layout_logo_help()} inline class="sm:col-span-2">
		<Toggle checked={layout.show_logo} onchange={(on: boolean) => (layout = { ...layout, show_logo: on })} label={m.reports_layout_logo()} {disabled} />
	</Field>

	<fieldset class="grid min-w-0 content-start gap-3 sm:col-span-2" {disabled}>
		<legend class="mb-0.5 block text-sm font-semibold text-ink">{m.reports_layout_scope()}</legend>
		<p class="-mt-1 text-[0.8125rem] text-ink-2">{scopeEmpty ? m.reports_layout_scope_all() : m.reports_layout_scope_help()}</p>

		{#if groups.length > 1 || (groups.length === 1 && groups[0] !== UNGROUPED_GROUP)}
			<div class="grid gap-1.5">
				<span class="text-[0.8125rem] font-semibold text-ink-2">{m.reports_layout_scope_groups()}</span>
				<div class="flex flex-wrap gap-2">
					{#each groups as name (name)}
						<Chip pressed={scope.groups.includes(name)} onclick={() => toggleGroup(name)} {disabled}>{groupLabel(name)}</Chip>
					{/each}
				</div>
			</div>
		{/if}

		<div class="grid gap-1.5">
			<span class="text-[0.8125rem] font-semibold text-ink-2">{m.reports_layout_scope_monitors()}</span>
			<Combobox
				id={`${idPrefix}-scope`}
				options={monitorOptions}
				selected={scope.targets.map(String)}
				ontoggle={toggleMonitor}
				label={m.reports_layout_scope_search()}
				emptyText={m.reports_layout_scope_empty()}
				ungroupedLabel={m.reports_layout_scope_ungrouped()}
				summary={scope.targets.length > 0 ? m.reports_layout_scope_selected({ count: scope.targets.length }) : ''}
				{disabled}
			/>
			{#if scope.targets.length > 0}
				<ul class="flex flex-wrap gap-2" role="list">
					{#each scope.targets as id (id)}
						{@const target = byId.get(id)}
						<li class="inline-flex min-h-10 items-center gap-1 rounded-full border border-line bg-surface pl-3 text-[0.8125rem] text-ink">
							{target?.name ?? m.reports_layout_scope_unknown({ id })}
							<button
								type="button"
								class="inline-flex size-10 items-center justify-center rounded-full text-ink-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
								aria-label={m.reports_layout_scope_remove({ name: target?.name ?? String(id) })}
								onclick={() => toggleMonitor(String(id), false)}
								{disabled}
							>
								<X class="size-4" aria-hidden="true" />
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</fieldset>
</div>
