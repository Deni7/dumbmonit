<script lang="ts">
	/**
	 * Reports → "Availability report (SLA)": the report of a period
	 * you choose, built on the spot and downloaded as a PDF or as CSV files.
	 * Dates are inclusive and read in the chosen timezone; the default is the
	 * previous calendar month. "Customize" holds the PDF layout (template,
	 * sections, paper, title, notes, logo) and the monitors covered; the same
	 * options open the printable page of the report.
	 */
	import { Download, Printer } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { pad2 } from '#lib/format.js';
	import { withBase } from '#lib/paths.js';
	import { toApiError, type StatusPage } from '#lib/api/index.js';
	import {
		defaultReportLayout,
		downloadSlaReport,
		emptyScope,
		slaParams,
		type ReportLayout,
		type ReportScope,
		type SlaFormat,
		type SlaRequest
	} from '#lib/api/reports.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, Disclosure, ErrorNotice, Field } from '#lib/ui/index.js';
	import ReportLayoutFields from '#lib/components/reports/ReportLayoutFields.svelte';

	interface Props {
		pages: StatusPage[];
		defaultTimezone: string;
		timezones: string[];
	}

	let { pages, defaultTimezone, timezones }: Props = $props();

	/** `YYYY-MM-DD` of a local date, without going through UTC (which shifts the day). */
	function day(date: Date): string {
		return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
	}

	const now = new Date();
	const lastMonthFirst = new Date(now.getFullYear(), now.getMonth() - 1, 1);
	const lastMonthLast = new Date(now.getFullYear(), now.getMonth(), 0);

	let from = $state(day(lastMonthFirst));
	let to = $state(day(lastMonthLast));
	let timezone = $state(defaultTimezone);
	let format = $state<SlaFormat>('pdf');
	let objective = $state('');
	let pageId = $state('');
	// MSP mode: the client the report is about. An administrator may leave it on
	// "all clients"; a member of several clients has to pick one.
	const clients = $derived(auth.mspOn ? (auth.msp?.clients ?? []) : []);
	const mspAdmin = $derived(auth.msp?.admin === true);
	let clientId = $state(auth.currentClient === null ? '' : String(auth.currentClient));
	const clientValue = $derived(clientId !== '' ? clientId : mspAdmin ? '' : String(clients[0]?.id ?? ''));
	let busy = $state(false);
	let error = $state<unknown>(null);
	let layout = $state<ReportLayout>(defaultReportLayout());
	let scope = $state<ReportScope>(emptyScope());
	let customizing = $state(false);

	const templateLabel = $derived(layout.template === 'summary' ? m.reports_layout_template_summary() : m.reports_layout_template_detailed());
	const paperLabel = $derived(layout.paper === 'letter' ? m.reports_layout_paper_letter() : m.reports_layout_paper_a4());

	/** What the form asks for, in the shape of the API. */
	function requestOf(wanted: SlaFormat): SlaRequest {
		const goal = Number(objective.replace(',', '.'));
		return {
			from,
			to,
			tz: timezone.trim() || undefined,
			format: wanted,
			objective: objective.trim() !== '' && goal > 0 ? goal : undefined,
			page: pageId === '' ? undefined : Number(pageId),
			client: clientValue === '' ? undefined : Number(clientValue),
			layout,
			scope
		};
	}

	/** The printable page of the same report, in the app. */
	const printHref = $derived.by(() => {
		const params = slaParams(requestOf('json'));
		params.delete('format');
		return withBase(`/reports/sla?${params}`);
	});

	const FORMATS: { value: SlaFormat; label: string }[] = [
		{ value: 'pdf', label: m.settings_sla_format_pdf() },
		{ value: 'csv', label: m.settings_sla_format_csv() },
		{ value: 'incidents_csv', label: m.settings_sla_format_incidents() }
	];

	function lastMonth() {
		from = day(lastMonthFirst);
		to = day(lastMonthLast);
	}

	function thisMonth() {
		from = day(new Date(now.getFullYear(), now.getMonth(), 1));
		to = day(now);
	}

	/** True once the last submit went through (the button confirms in green). */
	let downloadedOk = $state(false);
	async function download(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		busy = true;
		downloadedOk = false;
		try {
			const { blob, filename } = await downloadSlaReport(requestOf(format));
			const url = URL.createObjectURL(blob);
			const link = document.createElement('a');
			link.href = url;
			link.download = filename;
			document.body.append(link);
			link.click();
			link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 10_000);
			downloadedOk = true;
		} catch (cause) {
			error = toApiError(cause);
		} finally {
			busy = false;
		}
	}
</script>

<form class="grid gap-4 sm:grid-cols-2" onsubmit={download} novalidate>
	<Field label={m.settings_sla_from()} for="sla-from" help={m.settings_sla_period_help()}>
		<input id="sla-from" type="date" class="input" bind:value={from} max={to} required disabled={busy} />
	</Field>
	<Field label={m.settings_sla_to()} for="sla-to">
		<input id="sla-to" type="date" class="input" bind:value={to} min={from} required disabled={busy} />
	</Field>
	<div class="flex flex-wrap gap-2 sm:col-span-2">
		<Button type="button" variant="ghost" size="sm" onclick={lastMonth} disabled={busy}>{m.settings_sla_last_month()}</Button>
		<Button type="button" variant="ghost" size="sm" onclick={thisMonth} disabled={busy}>{m.settings_sla_this_month()}</Button>
	</div>

	<Field label={m.settings_report_timezone()} for="sla-tz">
		<input id="sla-tz" type="text" class="input font-mono" list="sla-tz-list" bind:value={timezone} autocomplete="off" spellcheck="false" disabled={busy} />
		<datalist id="sla-tz-list">
			{#each timezones as zone (zone)}<option value={zone}></option>{/each}
		</datalist>
	</Field>
	<Field label={m.settings_sla_format()} for="sla-format">
		<select id="sla-format" class="input" bind:value={format} disabled={busy}>
			{#each FORMATS as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
		</select>
	</Field>
	<Field label={m.settings_report_objective()} for="sla-objective" help={m.settings_report_objective_help()}>
		<input id="sla-objective" type="text" inputmode="decimal" class="input" bind:value={objective} placeholder="99.9" autocomplete="off" disabled={busy} />
	</Field>
	<Field label={m.settings_report_brand()} for="sla-brand" help={m.settings_report_brand_help()}>
		<select id="sla-brand" class="input" bind:value={pageId} disabled={busy}>
			<option value="">{m.settings_report_brand_none()}</option>
			{#each pages as statusPage (statusPage.id)}<option value={String(statusPage.id)}>{statusPage.title}</option>{/each}
		</select>
	</Field>

	{#if clients.length > 0}
		<Field label={m.reports_sla_client()} for="sla-client" help={m.reports_sla_client_help()}>
			<select id="sla-client" class="input" bind:value={clientId} disabled={busy}>
				{#if mspAdmin}<option value="">{m.reports_sla_client_all()}</option>{/if}
				{#each clients as client (client.id)}<option value={String(client.id)}>{client.name}</option>{/each}
			</select>
		</Field>
	{/if}

	<Disclosure
		title={m.reports_layout_customize()}
		summary={m.reports_layout_customize_summary({ template: templateLabel, paper: paperLabel })}
		bind:open={customizing}
		class="sm:col-span-2"
	>
		<div class="border-t border-line p-4">
			<ReportLayoutFields bind:layout bind:scope idPrefix="sla" disabled={busy} />
		</div>
	</Disclosure>

	<div class="flex flex-wrap items-center gap-2 sm:col-span-2">
		<Button type="submit" variant="primary" loading={busy} success={downloadedOk} disabled={!from || !to}>
			<Download class="size-4" aria-hidden="true" />
			{m.settings_sla_download()}
		</Button>
		<Button variant="secondary" href={printHref} disabled={!from || !to}>
			<Printer class="size-4" aria-hidden="true" />
			{m.reports_sla_open_printable()}
		</Button>
	</div>
</form>

{#if error}
	<div class="mt-3" aria-live="polite"><ErrorNotice {error} title={m.settings_sla_error()} /></div>
{/if}
