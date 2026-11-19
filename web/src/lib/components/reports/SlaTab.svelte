<script lang="ts">
	/**
	 * Status › SLA: the availability report of a period. Set what it covers and
	 * how it reads (period, monitors, template and options), then take it out as
	 * a PDF, print it (a printable sheet opens in a new tab), get the CSV files
	 * or schedule it. The report is never shown here: the form takes the whole
	 * page. The options live in the address (`?tab=sla&from=…&to=…`, the query
	 * of `GET /api/reports/sla`), so a setup is linkable; "Schedule" hands them
	 * to a new scheduled report in Status › Reports.
	 */

	import { untrack } from 'svelte';
	import { ChevronDown, Download, FileSpreadsheet, Printer, Repeat } from 'lucide-svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { m } from '#lib/paraglide/messages.js';
	import { listStatusPages, toApiError, type StatusPage } from '#lib/api/index.js';
	import {
		defaultReportLayout,
		downloadSlaReport,
		emptyScope,
		slaParams,
		slaRequestFrom,
		type ReportLayout,
		type ReportScope,
		type SlaFormat,
		type SlaRequest
	} from '#lib/api/reports.js';
	import { withBase } from '#lib/paths.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { presetRange, todayIso } from '#lib/ui/calendar.js';
	import { Button, DateRangePicker, ErrorNotice, Field, Menu, Panel, Plate } from '#lib/ui/index.js';
	import MonitorPicker from '#lib/components/reports/MonitorPicker.svelte';
	import ReportLayoutFields from '#lib/components/reports/ReportLayoutFields.svelte';

	/** Longest period the server builds (a leap year). */
	const MAX_DAYS = 366;

	const clients = $derived(auth.mspOn ? (auth.msp?.clients ?? []) : []);
	const mspAdmin = $derived(auth.msp?.admin === true);
	/** An administrator, or in MSP mode a member of a client (who reads that client's report). */
	const allowed = $derived(auth.isAdmin || clients.length > 0);

	const browserZone = (() => {
		try {
			return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
		} catch {
			return 'UTC';
		}
	})();
	const timezones = (() => {
		try {
			const list = (Intl as unknown as { supportedValuesOf?: (key: string) => string[] }).supportedValuesOf?.('timeZone');
			return list ? [...new Set(['UTC', ...list])] : ['UTC'];
		} catch {
			return ['UTC'];
		}
	})();

	// ---- The form, started from the address -----------------------------------

	const asked = untrack(() => slaRequestFrom(page.url.searchParams));
	const lastMonth = presetRange('last_month', todayIso());

	let from = $state(asked.from ?? lastMonth.from);
	let to = $state(asked.to ?? (asked.from ? todayIso() : lastMonth.to));
	let timezone = $state(asked.tz ?? browserZone);
	let objective = $state(asked.objective === undefined ? '' : String(asked.objective));
	let pageId = $state(asked.page === undefined ? '' : String(asked.page));
	let clientId = $state(
		asked.client !== undefined ? String(asked.client) : untrack(() => (auth.currentClient === null ? '' : String(auth.currentClient)))
	);
	let layout = $state<ReportLayout>(asked.layout ?? defaultReportLayout());
	let scope = $state<ReportScope>(asked.scope ?? emptyScope());
	let scopeEmpty = $state(false);

	// A member of several clients has to pick one; an MSP administrator may leave "all clients".
	const clientValue = $derived(clientId !== '' ? clientId : mspAdmin || auth.isAdmin ? '' : String(clients[0]?.id ?? ''));

	let pages = $state<StatusPage[]>([]);
	$effect(() => {
		if (!auth.isAdmin) return;
		const controller = new AbortController();
		listStatusPages(controller.signal)
			.then((list) => (pages = Array.isArray(list) ? list : []))
			.catch(() => (pages = []));
		return () => controller.abort();
	});

	// ---- What is asked ------------------------------------------------------------

	const goal = $derived(Number(objective.trim().replace(',', '.')));
	const objectiveError = $derived(objective.trim() !== '' && !(goal > 0 && goal <= 100) ? m.settings_report_objective_invalid() : null);

	const periodError = $derived.by(() => {
		if (!from || !to) return m.reports_sla_period_missing();
		const days = Math.round((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / 86_400_000) + 1;
		if (days > MAX_DAYS) return m.reports_sla_period_too_long({ max: MAX_DAYS });
		if (from > todayIso()) return m.reports_sla_period_future();
		return null;
	});

	const request = $derived<SlaRequest>({
		from,
		to,
		tz: timezone.trim() || undefined,
		format: 'json',
		objective: objective.trim() !== '' && !objectiveError ? goal : undefined,
		page: pageId === '' ? undefined : Number(pageId),
		client: clientValue === '' ? undefined : Number(clientValue),
		layout,
		scope
	});
	const ready = $derived(allowed && !periodError && !scopeEmpty);

	/** The options as a query string (no format): the address of this page and of the schedule hand-over. */
	const query = $derived.by(() => {
		const params = slaParams(request);
		params.delete('format');
		return params.toString();
	});

	// The address follows the form, so a setup can be linked or reloaded as it is.
	$effect(() => {
		const address = query;
		untrack(() => {
			const url = new URL(page.url);
			url.search = `tab=sla${address ? `&${address}` : ''}`;
			if (url.search === page.url.search) return;
			try {
				replaceState(url, page.state);
			} catch {
				// The router is not ready yet: the address catches up on the next change.
			}
		});
	});

	// ---- Files ----------------------------------------------------------------------

	let downloading = $state<SlaFormat | null>(null);
	let downloaded = $state(false);
	let downloadError = $state<unknown>(null);

	async function download(format: SlaFormat) {
		downloading = format;
		downloaded = false;
		downloadError = null;
		try {
			const { blob, filename } = await downloadSlaReport({ ...request, format });
			const url = URL.createObjectURL(blob);
			const link = document.createElement('a');
			link.href = url;
			link.download = filename;
			document.body.append(link);
			link.click();
			link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 10_000);
			if (format === 'pdf') downloaded = true;
		} catch (cause) {
			downloadError = toApiError(cause);
		} finally {
			downloading = null;
		}
	}

	const scheduleHref = $derived(`/status?tab=reports&schedule=sla&${query}`);

	/** The printable sheet opens in its own tab: it builds the report and opens the print dialog. */
	function printSheet() {
		window.open(withBase(`/sla/print?${query}`), '_blank', 'noopener');
	}

	const menuItem =
		'flex min-h-10 w-full items-center gap-2 rounded-md px-3 text-left text-sm text-ink hover:bg-surface focus-visible:outline-2 focus-visible:outline-signal disabled:opacity-50';
</script>

<div class="sla-page">
	<p class="mb-4 max-w-2xl text-sm text-ink-2 max-sm:hidden">{m.reports_sla_page_description()}</p>

	{#if !allowed}
		<Plate tone="ghost" label={auth.readOnlyLabel} size="md" />
		<p class="mt-3 text-sm text-ink-2">{m.reports_admin_only()}</p>
	{:else}
		{#if downloadError}
			<div class="mb-4" aria-live="polite"><ErrorNotice error={downloadError} title={m.reports_print_download_error()} /></div>
		{/if}

		<div class="grid items-start gap-6 lg:grid-cols-2">
			<div class="grid gap-6">
			<Panel title={m.reports_sla_period_title()}>
				<div class="grid gap-4">
					<Field label={m.reports_sla_period()} for="sla-period" error={periodError} help={m.settings_sla_period_help()}>
						<DateRangePicker id="sla-period" bind:from bind:to max={todayIso()} invalid={periodError !== null} label={m.reports_sla_period()} />
					</Field>
					<Field label={m.settings_report_timezone()} for="sla-tz">
						<input id="sla-tz" type="text" class="input font-mono" list="sla-tz-list" bind:value={timezone} autocomplete="off" spellcheck="false" />
						<datalist id="sla-tz-list">
							{#each timezones as zone (zone)}<option value={zone}></option>{/each}
						</datalist>
					</Field>
					<Field label={m.settings_report_objective()} for="sla-objective" error={objectiveError} help={m.settings_report_objective_help()}>
						<input
							id="sla-objective"
							type="text"
							inputmode="decimal"
							class="input"
							bind:value={objective}
							placeholder="99.9"
							autocomplete="off"
							aria-invalid={objectiveError ? 'true' : undefined}
						/>
					</Field>
				</div>
			</Panel>

			<Panel title={m.reports_sla_monitors_title()}>
				<div class="grid gap-4">
					{#if clients.length > 0}
						<Field label={m.reports_sla_client()} for="sla-client" help={m.reports_sla_client_help()}>
							<select id="sla-client" class="input" bind:value={clientId}>
								{#if mspAdmin || auth.isAdmin}<option value="">{m.reports_sla_client_all()}</option>{/if}
								{#each clients as client (client.id)}<option value={String(client.id)}>{client.name}</option>{/each}
							</select>
						</Field>
					{/if}
					<MonitorPicker bind:scope bind:empty={scopeEmpty} idPrefix="sla" />
				</div>
			</Panel>
			</div>

			<Panel title={m.reports_sla_layout_title()}>
				<div class="grid gap-4">
					{#if auth.isAdmin}
						<Field label={m.settings_report_brand()} for="sla-brand" help={m.settings_report_brand_help()}>
							<select id="sla-brand" class="input" bind:value={pageId}>
								<option value="">{m.settings_report_brand_none()}</option>
								{#each pages as statusPage (statusPage.id)}<option value={String(statusPage.id)}>{statusPage.title}</option>{/each}
							</select>
						</Field>
					{/if}
					<ReportLayoutFields
						bind:layout
						idPrefix="sla"
						pageId={pageId === '' ? null : Number(pageId)}
						{pages}
						clientId={clientValue === '' ? null : Number(clientValue)}
						wide={false}
					/>
				</div>
			</Panel>

		</div>

		<div class="mt-6 grid gap-2 border-t border-line pt-5">
			<div class="flex flex-wrap items-center gap-2">
				<Button variant="primary" onclick={() => void download('pdf')} loading={downloading === 'pdf'} success={downloaded} disabled={!ready}>
					<Download class="size-4" aria-hidden="true" />
					{m.reports_print_download()}
				</Button>
				<Button variant="secondary" onclick={printSheet} disabled={!ready}>
					<Printer class="size-4" aria-hidden="true" />
					{m.reports_print_print()}
				</Button>
				<Menu label={m.reports_sla_more_formats()} align="left">
					{#snippet trigger({ toggle, open })}
						<Button variant="secondary" onclick={toggle} aria-expanded={open} aria-haspopup="menu" disabled={!ready}>
							<FileSpreadsheet class="size-4" aria-hidden="true" />
							{m.reports_sla_csv()}
							<ChevronDown class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
					{#snippet children({ close })}
						<button type="button" role="menuitem" class={menuItem} disabled={downloading !== null} onclick={() => { close(); void download('csv'); }}>
							{m.settings_sla_format_csv()}
						</button>
						<button type="button" role="menuitem" class={menuItem} disabled={downloading !== null} onclick={() => { close(); void download('incidents_csv'); }}>
							{m.settings_sla_format_incidents()}
						</button>
					{/snippet}
				</Menu>
				{#if auth.isAdmin}
					<Button variant="secondary" href={scheduleHref} disabled={!ready}>
						<Repeat class="size-4" aria-hidden="true" />
						{m.reports_sla_schedule()}
					</Button>
				{/if}
			</div>
			{#if scopeEmpty}
				<p class="text-[0.8125rem] text-ink-2">{m.reports_scope_none_selected()}</p>
			{:else if periodError}
				<p class="text-[0.8125rem] text-ink-2">{periodError}</p>
			{/if}
		</div>
	{/if}
</div>
