<script lang="ts">
	/**
	 * The layout of an SLA report, chosen when it is built (on demand or on a
	 * schedule): template, sections, title, paper, notes and logo. The monitors
	 * it covers are `MonitorPicker`'s. The parent owns `layout` (bindable);
	 * switching the template resets the sections to that template's starting
	 * set, which the person can then change. Same field rhythm as the other
	 * settings forms: `Field` labels, 40px controls, a two-column grid that
	 * folds to one on a phone.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import type { StatusPage } from '#lib/api/index.js';
	import {
		MAX_REPORT_NOTES,
		MAX_REPORT_TITLE,
		REPORT_SECTION_KEYS,
		sectionsFor,
		withLogo,
		type ReportLayout,
		type ReportSections,
		type ReportTemplate
	} from '#lib/api/reports.js';
	import { Field, Segmented } from '#lib/ui/index.js';
	import ReportLogoField from './ReportLogoField.svelte';

	interface Props {
		layout: ReportLayout;
		/** Unique per form: ids of the controls. */
		idPrefix: string;
		/** Status page branding the report (for the logo choice). */
		pageId: number | null;
		pages: StatusPage[];
		/** The report's client; `undefined` when set elsewhere (a scheduled report's assignment). */
		clientId?: number | null;
		disabled?: boolean;
		/** Two columns from the small breakpoint on; off in a narrow side panel. */
		wide?: boolean;
		/** The uploaded report logo was replaced or removed. */
		onlogochanged?: () => void;
	}

	let { layout = $bindable(), idPrefix, pageId, pages, clientId, disabled = false, wide = true, onlogochanged }: Props = $props();

	const TEMPLATES = [
		{ value: 'detailed', label: m.reports_layout_template_detailed() },
		{ value: 'summary', label: m.reports_layout_template_summary() }
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

	function pickTemplate(next: string) {
		const template: ReportTemplate = next === 'summary' ? 'summary' : 'detailed';
		if (template === layout.template) return;
		layout = { ...layout, template, sections: sectionsFor(template) };
	}

	function setSection(key: keyof ReportSections, on: boolean) {
		layout = { ...layout, sections: { ...layout.sections, [key]: on } };
	}

	const cols = $derived(wide ? 'sm:grid-cols-2' : '');
	const span = $derived(wide ? 'sm:col-span-2' : '');
</script>

<div class={`grid gap-4 ${cols}`}>
	<Segmented
		legend={m.reports_layout_template()}
		options={TEMPLATES}
		value={layout.template}
		name={`${idPrefix}-template`}
		help={layout.template === 'summary' ? m.reports_layout_template_summary_help() : m.reports_layout_template_detailed_help()}
		{disabled}
		onchange={pickTemplate}
		class={span}
	/>

	<fieldset class={`grid min-w-0 content-start gap-1.5 ${span}`} {disabled}>
		<legend class="mb-1.5 block text-sm font-semibold text-ink">{m.reports_layout_sections()}</legend>
		<div class={`grid gap-x-4 ${cols}`}>
			{#each REPORT_SECTION_KEYS as key (key)}
				<label class="flex min-h-10 cursor-pointer items-center gap-2.5 text-sm text-ink">
					<input
						type="checkbox"
						class="size-4 shrink-0 accent-[var(--c-ink)]"
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
			<option value="a4">{m.reports_layout_paper_a4()}</option>
			<option value="letter">{m.reports_layout_paper_letter()}</option>
		</select>
	</Field>

	{#if layout.sections.notes}
		<Field label={m.reports_layout_notes()} for={`${idPrefix}-notes`} help={m.reports_layout_notes_help({ max: MAX_REPORT_NOTES })} class={span}>
			<textarea
				id={`${idPrefix}-notes`}
				class="input"
				value={layout.notes}
				oninput={(event) => (layout = { ...layout, notes: event.currentTarget.value })}
				maxlength={MAX_REPORT_NOTES}
				{disabled}
			></textarea>
		</Field>
	{/if}

	<div class={span}>
		<ReportLogoField
			value={layout.show_logo ? layout.logo : 'none'}
			idPrefix={idPrefix}
			{clientId}
			{pageId}
			{pages}
			{disabled}
			onchange={(logo) => (layout = withLogo(layout, logo))}
			{onlogochanged}
		/>
	</div>
</div>
