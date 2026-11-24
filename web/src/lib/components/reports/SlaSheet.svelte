<script lang="ts">
	/**
	 * The availability (SLA) report drawn as a sheet of paper: the same data,
	 * template and sections as the PDF. On screen it sits on the canvas;
	 * printed, only the sheet comes out, black on white, with table headers
	 * repeated on every page and no background the printer could drop. Status
	 * is always a word, never a colour alone. `logoVersion` busts the cache of
	 * the logo after it was replaced.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import { formatDuration } from '#lib/format.js';
	import { UNGROUPED_GROUP, chartFloor, chartShare, type SlaIncident, type SlaReport } from '#lib/api/reports.js';

	interface Props {
		report: SlaReport;
		logoVersion?: string;
	}

	let { report, logoVersion = '' }: Props = $props();

	const MAX_CHART_BARS = 15;
	const MAX_SUMMARY_MONITORS = 25;

	// ---- Formatting --------------------------------------------------------

	const pct = (value: number | null) => (value === null ? m.reports_print_no_value() : `${Math.min(100, Math.max(0, value)).toFixed(3)}%`);
	const ms = (value: number | null) => (value === null ? m.reports_print_no_value() : `${Math.round(value)} ms`);

	/** A date in the report's timezone (not the browser's): the period is read there. */
	function when(iso: string | null): string {
		if (!iso) return m.reports_print_no_value();
		const date = new Date(iso);
		try {
			return new Intl.DateTimeFormat(getLocale(), {
				dateStyle: 'medium',
				timeStyle: 'short',
				timeZone: report.timezone || 'UTC'
			}).format(date);
		} catch {
			return date.toISOString().slice(0, 16).replace('T', ' ');
		}
	}

	/** The server's status words, in the reader's language. */
	function statusLabel(word: string): string {
		switch (word) {
			case 'Met':
				return m.reports_print_status_met();
			case 'Missed':
				return m.reports_print_status_missed();
			case 'Healthy':
				return m.reports_print_status_healthy();
			case 'Degraded':
				return m.reports_print_status_degraded();
			case 'Poor':
				return m.reports_print_status_poor();
			default:
				return m.reports_print_status_no_data();
		}
	}
	const bad = (word: string) => word === 'Missed' || word === 'Poor' || word === 'Degraded';

	const groupLabel = (name: string) => (name === UNGROUPED_GROUP ? m.reports_layout_scope_ungrouped() : name);

	function length(incident: SlaIncident): string {
		const span = formatDuration(incident.duration_secs);
		return incident.ended_at === null ? m.reports_print_ongoing({ duration: span }) : span;
	}

	// ---- What the template shows --------------------------------------------

	const sections = $derived(report.layout.sections);
	const summary = $derived(report.layout.template === 'summary');

	const bars = $derived.by(() => {
		const source = report.grouped
			? report.groups.map((g) => ({ name: groupLabel(g.name), value: g.uptime }))
			: report.monitors.map((d) => ({ name: d.name, value: d.uptime }));
		return source.filter((b): b is { name: string; value: number } => b.value !== null);
	});
	const shownBars = $derived(bars.slice(0, MAX_CHART_BARS));
	const floor = $derived(chartFloor(shownBars.map((b) => b.value), report.objective));
	const objectiveShare = $derived(report && report.objective !== null && report.objective > floor ? chartShare(report.objective, floor) : null);

	// An "Objective" column only when a group or a monitor has an objective of its own.
	const ownGroups = $derived(report.groups.some((g) => g.own_objective));
	const ownMonitors = $derived(report.monitors.some((d) => d.own_objective));
	const metCount = $derived(report.monitors.filter((d) => d.uptime !== null && d.status === 'Met').length);
	const measuredCount = $derived(report.monitors.filter((d) => d.uptime !== null).length);

	const monitorRows = $derived(summary ? report.monitors.slice(0, MAX_SUMMARY_MONITORS) : report.monitors);
	const incidentRows = $derived(summary ? report.incidents.filter((i) => i.major) : report.incidents);
	const timed = $derived(
		report
			? report.monitors
					.filter((d) => d.latency_avg_ms !== null)
					.sort((a, b) => (b.latency_p95_ms ?? 0) - (a.latency_p95_ms ?? 0) || a.name.localeCompare(b.name))
			: []
	);
	const maintenanceTotal = $derived(
		report ? report.maintenance.reduce((sum, w) => sum + (Date.parse(w.ends_at) - Date.parse(w.starts_at)) / 1000, 0) : 0
	);
	const slowest = $derived.by(() => {
		const names = timed.slice(0, 3).map((d) => m.reports_print_slowest_item({ name: d.name, p95: ms(d.latency_p95_ms) }));
		try {
			return new Intl.ListFormat(getLocale(), { type: 'conjunction' }).format(names);
		} catch {
			return names.join(', ');
		}
	});
</script>

<article class="sla-sheet mx-auto w-full max-w-[52rem] rounded-[var(--radius-card)] border border-line bg-surface px-4 py-6 text-ink sm:px-10 sm:py-10">
	<header class="sla-keep flex items-start justify-between gap-6 border-b-2 border-ink pb-5">
		<div class="grid min-w-0 gap-1">
			<p class="text-2xl font-bold tracking-tight text-ink sm:text-3xl">{report.brand}</p>
			<h2 class="text-lg font-semibold text-ink-2">{report.title}</h2>
			<p class="text-sm text-ink">{report.period_label}</p>
		</div>
		{#if report.logo_url}
			<img src={`/api${report.logo_url}${logoVersion ? `?v=${encodeURIComponent(logoVersion)}` : ''}`} alt={m.reports_print_logo_alt({ brand: report.brand })} class="h-14 max-w-40 shrink-0 object-contain sm:h-16 sm:max-w-48" />
		{/if}
	</header>

	{#if sections.summary}
		<section class="sla-keep mt-7" aria-labelledby="sla-summary">
			<h3 id="sla-summary" class="sr-only">{m.reports_print_summary()}</h3>
			<div class="grid gap-x-8 gap-y-5 sm:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,1fr)]">
				<div class="grid content-start gap-1">
					<p class="text-sm font-semibold text-ink-2">{m.reports_print_availability()}</p>
					<p class="sla-figure text-4xl font-bold tnum text-ink sm:text-5xl">{pct(report.fleet_uptime)}</p>
					<p class={`text-sm ${bad(report.fleet_status) ? 'font-bold text-warning-ink' : 'text-ink-2'}`}>
						{#if report.objective !== null}
							{m.reports_print_objective_line({ objective: pct(report.objective), status: statusLabel(report.fleet_status) })}
						{:else}
							{m.reports_print_previous_line({ status: statusLabel(report.fleet_status), value: pct(report.fleet_previous_uptime) })}
						{/if}
					</p>
				</div>
				<div class="grid content-start gap-1 border-line sm:border-l sm:pl-6">
					<p class="text-sm font-semibold text-ink-2">{m.reports_print_downtime()}</p>
					<p class="text-2xl font-bold tnum text-ink">{formatDuration(report.downtime_secs)}</p>
					<p class="text-[0.8125rem] text-ink-2">{m.reports_print_downtime_sub()}</p>
				</div>
				<div class="grid content-start gap-1 border-line sm:border-l sm:pl-6">
					<p class="text-sm font-semibold text-ink-2">{m.reports_print_incidents()}</p>
					<p class="text-2xl font-bold tnum text-ink">{report.incident_count}</p>
					<p class="text-[0.8125rem] text-ink-2">{m.reports_print_incidents_previous({ count: report.previous_incident_count })}</p>
				</div>
			</div>
			{#if report.objective !== null}
				<p class="mt-4 text-sm text-ink-2">{m.reports_print_objective_met_by({ objective: pct(report.objective), met: metCount, total: measuredCount })}</p>
			{/if}
			{#if !report.availability_known}
				<p class="mt-2 text-sm text-ink-2">{m.reports_print_no_tsdb()}</p>
			{/if}
		</section>
	{/if}

	{#if sections.chart}
		<section class="sla-keep mt-9" aria-labelledby="sla-chart">
			<h3 id="sla-chart" class="sla-heading">{report.grouped ? m.reports_print_chart_groups() : m.reports_print_chart_monitors()}</h3>
			{#if shownBars.length === 0}
				<p class="text-sm text-ink-2">{m.reports_print_chart_empty()}</p>
			{:else}
				<ul class="grid gap-1.5" role="list">
					{#each shownBars as bar, index (index)}
						<li class="grid grid-cols-[minmax(0,9rem)_minmax(0,1fr)_4.75rem] items-center gap-3 text-sm sm:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_5rem]">
							<span class="truncate text-ink" title={bar.name}>{bar.name}</span>
							<span class="relative block h-3 border-l border-ink">
								<svg class="absolute inset-0 h-full w-full" viewBox="0 0 100 10" preserveAspectRatio="none" aria-hidden="true">
									<rect x="0" y="1" width={chartShare(bar.value, floor) * 100} height="8" class="sla-bar" />
								</svg>
								{#if objectiveShare !== null}
									<span class="absolute -inset-y-1 border-l border-dashed border-ink" style={`left: ${objectiveShare * 100}%`} aria-hidden="true"></span>
								{/if}
							</span>
							<span class="text-right font-semibold tnum text-ink">{pct(bar.value)}</span>
						</li>
					{/each}
				</ul>
				<p class="mt-2 text-[0.8125rem] text-ink-2">
					{m.reports_print_chart_scale({ floor: `${floor}%` })}
					{#if objectiveShare !== null && report.objective !== null}{' '}{m.reports_print_chart_objective({ objective: pct(report.objective) })}{/if}
					{#if bars.length > shownBars.length}{' '}{m.reports_print_chart_more({ count: bars.length - shownBars.length })}{/if}
				</p>
			{/if}
		</section>
	{/if}

	{#if sections.monitors && report.grouped}
		<section class="mt-9" aria-labelledby="sla-groups">
			<h3 id="sla-groups" class="sla-heading">{m.reports_print_by_group()}</h3>
			<div class="sla-scroll">
				<table class="sla-table">
					<thead>
						<tr>
							<th scope="col">{m.reports_print_col_group()}</th>
							<th scope="col" class="num">{m.reports_print_col_monitors()}</th>
							<th scope="col" class="num">{m.reports_print_col_availability()}</th>
							{#if ownGroups}<th scope="col" class="num">{m.reports_print_col_objective()}</th>{/if}
							<th scope="col">{m.reports_print_col_status()}</th>
							<th scope="col" class="num">{m.reports_print_col_previous()}</th>
						</tr>
					</thead>
					<tbody>
						{#each report.groups as group (group.name)}
							<tr>
								<td>{groupLabel(group.name)}</td>
								<td class="num">{group.monitors}</td>
								<td class="num font-semibold">{pct(group.uptime)}</td>
								{#if ownGroups}<td class="num text-ink-2">{group.objective === null ? m.reports_print_no_value() : pct(group.objective)}</td>{/if}
								<td class:sla-bad={bad(group.status)}>{statusLabel(group.status)}</td>
								<td class="num text-ink-2">{pct(group.previous_uptime)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</section>
	{/if}

	{#if sections.monitors && !(summary && report.grouped)}
		<section class="mt-9" aria-labelledby="sla-monitors">
			<h3 id="sla-monitors" class="sla-heading">{m.reports_print_by_monitor()}</h3>
			{#if report.monitors.length === 0}
				<p class="text-sm text-ink-2">{m.reports_print_no_monitor()}</p>
			{:else}
				<div class="sla-scroll">
					<table class="sla-table">
						<thead>
							<tr>
								<th scope="col">{m.reports_print_col_monitor()}</th>
								<th scope="col">{m.reports_print_col_group()}</th>
								<th scope="col" class="num">{m.reports_print_col_availability()}</th>
								{#if ownMonitors}<th scope="col" class="num">{m.reports_print_col_objective()}</th>{/if}
								<th scope="col">{m.reports_print_col_status()}</th>
								<th scope="col" class="num">{m.reports_print_col_incidents()}</th>
								<th scope="col" class="num">{m.reports_print_col_downtime()}</th>
							</tr>
						</thead>
						<tbody>
							{#each monitorRows as monitor, index (index)}
								<tr>
									<td>{monitor.name}</td>
									<td class="text-ink-2">{groupLabel(monitor.group)}</td>
									<td class="num font-semibold">{pct(monitor.uptime)}</td>
									{#if ownMonitors}<td class="num text-ink-2">{monitor.objective === null ? m.reports_print_no_value() : pct(monitor.objective)}</td>{/if}
									<td class:sla-bad={bad(monitor.status)}>{statusLabel(monitor.status)}</td>
									<td class="num">{monitor.incidents}</td>
									<td class="num">{formatDuration(monitor.downtime_secs)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if report.monitors.length > monitorRows.length}
					<p class="mt-2 text-[0.8125rem] text-ink-2">{m.reports_print_more_monitors({ count: report.monitors.length - monitorRows.length })}</p>
				{/if}
			{/if}
		</section>
	{/if}

	{#if sections.incidents}
		<section class="mt-9" aria-labelledby="sla-incidents">
			<h3 id="sla-incidents" class="sla-heading">{summary ? m.reports_print_major_incidents() : m.reports_print_incidents()}</h3>
			{#if incidentRows.length === 0}
				<p class="text-sm text-ink-2">{m.reports_print_no_incident()}</p>
			{:else}
				<div class="sla-scroll">
					<table class="sla-table">
						<thead>
							<tr>
								<th scope="col">{m.reports_print_col_monitor()}</th>
								<th scope="col">{m.reports_print_col_started()}</th>
								<th scope="col">{m.reports_print_col_duration()}</th>
								<th scope="col">{m.reports_print_col_severity()}</th>
								<th scope="col">{m.reports_print_col_rule()}</th>
							</tr>
						</thead>
						<tbody>
							{#each incidentRows as incident, index (index)}
								<tr>
									<td>{incident.monitor}</td>
									<td class="whitespace-nowrap">{incident.began_before_period ? m.reports_print_before_period() : when(incident.started_at)}</td>
									<td class="whitespace-nowrap">{length(incident)}</td>
									<td class:font-semibold={incident.severity === 'critical'}>{incident.severity}</td>
									<td>{incident.rule}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if summary && report.incident_count > incidentRows.length}
					<p class="mt-2 text-[0.8125rem] text-ink-2">{m.reports_print_incidents_total({ count: report.incident_count })}</p>
				{/if}
			{/if}
		</section>
	{/if}

	{#if sections.maintenance}
		<section class="mt-9" aria-labelledby="sla-maintenance">
			<h3 id="sla-maintenance" class="sla-heading">{m.reports_print_maintenance()}</h3>
			{#if report.maintenance.length === 0}
				<p class="text-sm text-ink-2">{m.reports_print_no_maintenance()}</p>
			{:else if summary}
				<p class="text-sm text-ink">{m.reports_print_maintenance_total({ count: report.maintenance.length, duration: formatDuration(maintenanceTotal) })}</p>
			{:else}
				<div class="sla-scroll">
					<table class="sla-table">
						<thead>
							<tr>
								<th scope="col">{m.reports_print_col_window()}</th>
								<th scope="col">{m.reports_print_col_monitor()}</th>
								<th scope="col">{m.reports_print_col_starts()}</th>
								<th scope="col">{m.reports_print_col_ends()}</th>
								<th scope="col">{m.reports_print_col_repeats()}</th>
							</tr>
						</thead>
						<tbody>
							{#each report.maintenance as window, index (index)}
								<tr>
									<td>{window.name}</td>
									<td class="text-ink-2">{window.monitor ?? m.reports_print_several_monitors()}</td>
									<td class="whitespace-nowrap">{when(window.starts_at)}</td>
									<td class="whitespace-nowrap">{when(window.ends_at)}</td>
									<td>{window.recurring ? m.reports_print_yes() : m.reports_print_no()}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>
	{/if}

	{#if sections.response_times}
		<section class="mt-9" aria-labelledby="sla-response">
			<h3 id="sla-response" class="sla-heading">{m.reports_print_response_times()}</h3>
			{#if timed.length === 0}
				<p class="text-sm text-ink-2">{m.reports_print_no_response()}</p>
			{:else if summary}
				<p class="text-sm text-ink">{m.reports_print_slowest({ count: timed.length, list: slowest })}</p>
			{:else}
				<div class="sla-scroll">
					<table class="sla-table">
						<thead>
							<tr>
								<th scope="col">{m.reports_print_col_monitor()}</th>
								<th scope="col">{m.reports_print_col_group()}</th>
								<th scope="col" class="num">{m.reports_print_col_mean()}</th>
								<th scope="col" class="num">{m.reports_print_col_p95()}</th>
							</tr>
						</thead>
						<tbody>
							{#each timed as monitor, index (index)}
								<tr>
									<td>{monitor.name}</td>
									<td class="text-ink-2">{groupLabel(monitor.group)}</td>
									<td class="num">{ms(monitor.latency_avg_ms)}</td>
									<td class="num font-semibold">{ms(monitor.latency_p95_ms)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>
	{/if}

	{#if sections.notes && report.layout.notes.trim() !== ''}
		<section class="sla-keep mt-9" aria-labelledby="sla-notes">
			<h3 id="sla-notes" class="sla-heading">{m.reports_print_notes()}</h3>
			<p class="max-w-[68ch] text-[0.9375rem] leading-relaxed whitespace-pre-line text-ink">{report.layout.notes}</p>
		</section>
	{/if}

	<section class="sla-keep mt-9" aria-labelledby="sla-method">
		<h3 id="sla-method" class="sla-heading">{m.reports_print_method_title()}</h3>
		<p class="max-w-[68ch] text-[0.8125rem] leading-relaxed text-ink-2">{m.reports_print_method()}</p>
	</section>

	<footer class="mt-9 border-t border-line pt-3 text-[0.75rem] text-ink-2">
		{m.reports_print_generated({ when: when(report.generated_at) })}
	</footer>
</article>

<style>
	.sla-heading {
		margin-bottom: 0.75rem;
		font-size: 1.0625rem;
		font-weight: 700;
		color: var(--c-ink);
		break-after: avoid;
	}
	.sla-figure {
		letter-spacing: -0.02em;
		line-height: 1;
	}
	.sla-bar {
		fill: var(--c-ink-2);
		stroke: var(--c-ink);
		stroke-width: 0.4;
		vector-effect: non-scaling-stroke;
	}
	/* Wide tables scroll inside their box on a phone, never the page. */
	.sla-scroll {
		overflow-x: auto;
	}
	.sla-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
		font-variant-numeric: tabular-nums;
	}
	.sla-table th {
		padding: 0.375rem 0.625rem 0.375rem 0;
		border-bottom: 1.5px solid var(--c-ink);
		text-align: left;
		font-weight: 700;
		white-space: nowrap;
		color: var(--c-ink);
	}
	.sla-table td {
		padding: 0.375rem 0.625rem 0.375rem 0;
		border-bottom: 1px solid var(--c-line);
		vertical-align: top;
	}
	.sla-table .num {
		text-align: right;
	}
	/* A status that needs attention is bold as well as tinted: it survives black and white. */
	.sla-bad {
		font-weight: 700;
		color: var(--c-warning-ink);
	}

	@media print {
		/* Only the sheet is printed: the app around it is hidden but keeps no space. */
		:global(body *) {
			visibility: hidden;
		}
		.sla-sheet,
		.sla-sheet :global(*) {
			visibility: visible;
		}
		.sla-sheet {
			position: absolute;
			top: 0;
			left: 0;
			width: 100%;
			max-width: none;
			margin: 0;
			padding: 0;
			border: 0;
			border-radius: 0;
			background: #ffffff;
			font-size: 10.5pt;
			/* Paper is white whatever the screen theme: day ink, darker lines. */
			--c-ink: #111111;
			--c-ink-2: #444444;
			--c-ink-3: #666666;
			--c-line: #9a9a9a;
			--c-warning-ink: #7a1f14;
			--c-surface: #ffffff;
			color: #111111;
		}
		:global(html),
		:global(body) {
			background: #ffffff !important;
		}
		.sla-scroll {
			overflow: visible;
		}
		.sla-table thead {
			display: table-header-group;
		}
		.sla-table tr {
			break-inside: avoid;
		}
		.sla-keep {
			break-inside: avoid;
		}
		.sla-table {
			font-size: 9pt;
		}
	}
</style>
