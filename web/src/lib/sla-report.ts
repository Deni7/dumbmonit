/**
 * SLA report options and data: the PDF layout (template, paper, sections,
 * title, notes, logo), the scope, the query string of `GET /api/reports/sla`
 * and what the printable page draws. Pure: no network, no messages, so tests
 * load it directly. Mirrors `crates/server/src/reports/layout.rs` and
 * `SlaReportView` in `crates/server/src/api/reports.rs`.
 */

/** `detailed`: every monitor and incident; `summary`: one or two pages. Mirrors `reports::layout::Template`. */
export type ReportTemplate = 'detailed' | 'summary';

export type ReportPaper = 'a4' | 'letter';

/**
 * Where the logo at the top comes from. `auto`: the client's (MSP mode), else
 * the status page's; `custom`: the logo uploaded for reports. Mirrors
 * `reports::layout::LogoSource`.
 */
export type ReportLogo = 'auto' | 'client' | 'page' | 'custom' | 'none';

export const REPORT_LOGOS: ReportLogo[] = ['auto', 'client', 'page', 'custom', 'none'];

const readLogo = (raw: unknown): ReportLogo | null =>
	raw === 'false' ? 'none' : raw === 'true' ? 'auto' : REPORT_LOGOS.includes(raw as ReportLogo) ? (raw as ReportLogo) : null;

/** Sections of the SLA report, in report order. Mirrors `reports::layout::Sections`. */
export interface ReportSections {
	summary: boolean;
	chart: boolean;
	monitors: boolean;
	incidents: boolean;
	maintenance: boolean;
	response_times: boolean;
	notes: boolean;
}

export const REPORT_SECTION_KEYS: (keyof ReportSections)[] = [
	'summary',
	'chart',
	'monitors',
	'incidents',
	'maintenance',
	'response_times',
	'notes'
];

/** How the SLA PDF is laid out. Mirrors `reports::layout::Layout`. */
export interface ReportLayout {
	template: ReportTemplate;
	paper: ReportPaper;
	sections: ReportSections;
	/** Heading under the brand; `null`: "Availability report". 120 characters at most. */
	title: string | null;
	/** Printed in the Notes section. 2000 characters at most. */
	notes: string;
	/** Kept for layouts saved before `logo`: always `logo !== 'none'`. */
	show_logo: boolean;
	logo: ReportLogo;
}

/** The monitors listed and those of the groups listed (union); empty: everything. Mirrors `reports::layout::Scope`. */
export interface ReportScope {
	targets: number[];
	/** Group names; `Ungrouped` selects the monitors without a group. */
	groups: string[];
}

export const MAX_REPORT_TITLE = 120;
export const MAX_REPORT_NOTES = 2000;
/** Name the server gives the group of monitors without one. */
export const UNGROUPED_GROUP = 'Ungrouped';

/** Sections a template starts with: the summary leaves out the long tables nobody reads on one page. */
export function sectionsFor(template: ReportTemplate): ReportSections {
	return template === 'summary'
		? { summary: true, chart: true, monitors: true, incidents: true, maintenance: false, response_times: false, notes: true }
		: { summary: true, chart: true, monitors: true, incidents: true, maintenance: true, response_times: true, notes: true };
}

export function defaultReportLayout(): ReportLayout {
	return { template: 'detailed', paper: 'a4', sections: sectionsFor('detailed'), title: null, notes: '', show_logo: true, logo: 'auto' };
}

/** A stored layout with every field present (an old schedule may lack some). */
export function normaliseLayout(layout: Partial<ReportLayout> | null | undefined): ReportLayout {
	const base = defaultReportLayout();
	// An old layout only says whether a logo is shown; `false` wins.
	const logo = layout?.show_logo === false ? 'none' : (readLogo(layout?.logo) ?? 'auto');
	return {
		template: layout?.template === 'summary' ? 'summary' : 'detailed',
		paper: layout?.paper === 'letter' ? 'letter' : 'a4',
		sections: { ...base.sections, ...(layout?.sections ?? {}) },
		title: layout?.title ?? null,
		notes: layout?.notes ?? '',
		show_logo: logo !== 'none',
		logo
	};
}

export function emptyScope(): ReportScope {
	return { targets: [], groups: [] };
}

export type SlaFormat = 'pdf' | 'csv' | 'incidents_csv' | 'json';

export interface SlaRequest {
	/** First day included, `YYYY-MM-DD`; with `to` omitted, up to today. */
	from?: string;
	to?: string;
	/** IANA name; the period and the dates in the report follow it. */
	tz?: string;
	format: SlaFormat;
	objective?: number;
	/** Status page whose title heads the report. */
	page?: number;
	/** MSP mode: the client reported on (its targets, name and logo). */
	client?: number;
	/** PDF layout and scope; omitted: the detailed report of everything. */
	layout?: ReportLayout;
	scope?: ReportScope;
}

/** The same layout with `logo` and `show_logo` in step (what the editors hand out). */
export function withLogo(layout: ReportLayout, logo: ReportLogo): ReportLayout {
	return { ...layout, logo, show_logo: logo !== 'none' };
}

/**
 * Query string of `GET /api/reports/sla`: the layout travels as `template`,
 * `paper`, `sections` (comma list), `title`, `notes`, `logo` (left out when
 * `auto`); the scope as JSON.
 */
export function slaParams(request_: SlaRequest): URLSearchParams {
	const params = new URLSearchParams();
	const { layout, scope, ...rest } = request_;
	for (const [key, value] of Object.entries(rest)) {
		if (value !== undefined && value !== '') params.set(key, String(value));
	}
	if (layout) {
		params.set('template', layout.template);
		params.set('paper', layout.paper);
		const sections = REPORT_SECTION_KEYS.filter((key) => layout.sections[key]);
		// No section at all would read as "all of them": keep the summary.
		params.set('sections', sections.length > 0 ? sections.join(',') : 'summary');
		if (layout.title && layout.title.trim() !== '') params.set('title', layout.title.trim());
		if (layout.notes.trim() !== '') params.set('notes', layout.notes.trim());
		const logo = layout.show_logo ? layout.logo : 'none';
		if (logo !== 'auto') params.set('logo', logo);
	}
	if (scope && (scope.targets.length > 0 || scope.groups.length > 0)) {
		params.set('scope', JSON.stringify(scope));
	}
	return params;
}

/** Reads a request back from a query string (the printable page's address). */
export function slaRequestFrom(params: URLSearchParams): SlaRequest {
	const number = (key: string) => {
		const raw = params.get(key);
		const value = raw === null || raw === '' ? NaN : Number(raw);
		return Number.isFinite(value) ? value : undefined;
	};
	const layout = defaultReportLayout();
	if (params.get('template') === 'summary') layout.template = 'summary';
	if (params.get('paper') === 'letter') layout.paper = 'letter';
	const sections = params.get('sections');
	if (sections) {
		const wanted = new Set(sections.split(',').map((s) => s.trim()));
		for (const key of REPORT_SECTION_KEYS) layout.sections[key] = wanted.has(key);
	}
	layout.title = params.get('title') || null;
	layout.notes = params.get('notes') ?? '';
	layout.logo = readLogo(params.get('logo')) ?? 'auto';
	layout.show_logo = layout.logo !== 'none';
	let scope = emptyScope();
	try {
		const parsed: unknown = JSON.parse(params.get('scope') ?? 'null');
		if (parsed && typeof parsed === 'object') {
			const raw = parsed as Partial<ReportScope>;
			scope = {
				targets: Array.isArray(raw.targets) ? raw.targets.filter((id) => Number.isInteger(id)) : [],
				groups: Array.isArray(raw.groups) ? raw.groups.filter((g) => typeof g === 'string') : []
			};
		}
	} catch {
		// Not JSON: the whole estate.
	}
	return {
		from: params.get('from') ?? undefined,
		to: params.get('to') ?? undefined,
		tz: params.get('tz') ?? undefined,
		format: 'json',
		objective: number('objective'),
		page: number('page'),
		client: number('client'),
		layout,
		scope
	};
}

/** The SLA report as data, for the printable page. Mirrors `SlaReportView` in `api/reports.rs`. */
export interface SlaReport {
	brand: string;
	title: string;
	period_label: string;
	period_start: string;
	period_end: string;
	timezone: string;
	generated_at: string;
	objective: number | null;
	layout: ReportLayout;
	/** Address of the logo at the top, relative to `/api`; `null` without one. */
	logo_url: string | null;
	availability_known: boolean;
	fleet_uptime: number | null;
	fleet_previous_uptime: number | null;
	/** `Met`, `Missed`, `Healthy`, `Degraded`, `Poor` or `No data` (server words, English). */
	fleet_status: string;
	downtime_secs: number;
	incident_count: number;
	previous_incident_count: number;
	grouped: boolean;
	groups: SlaGroup[];
	monitors: SlaMonitor[];
	incidents: SlaIncident[];
	maintenance: SlaMaintenance[];
}

export interface SlaGroup {
	name: string;
	monitors: number;
	uptime: number | null;
	previous_uptime: number | null;
	status: string;
}

export interface SlaMonitor {
	name: string;
	group: string;
	uptime: number | null;
	previous_uptime: number | null;
	status: string;
	incidents: number;
	downtime_secs: number;
	latency_avg_ms: number | null;
	latency_p95_ms: number | null;
}

export interface SlaIncident {
	monitor: string;
	rule: string;
	severity: string;
	started_at: string;
	ended_at: string | null;
	duration_secs: number;
	began_before_period: boolean;
	/** Among the major incidents of the summary template. */
	major: boolean;
}

export interface SlaMaintenance {
	name: string;
	monitor: string | null;
	starts_at: string;
	ends_at: string;
	recurring: boolean;
}

/** Start of the chart scale: the largest step below every value and the objective. */
export function chartFloor(values: number[], objective: number | null): number {
	const lowest = Math.min(100, ...values, objective ?? 100);
	return [99.9, 99.5, 99, 98, 95, 90, 80, 50, 0].find((step) => step < lowest) ?? 0;
}

/** Position of a value on the chart, from 0 (the floor) to 1 (100%). */
export function chartShare(value: number, floor: number): number {
	if (floor >= 100) return 1;
	return (Math.min(100, Math.max(floor, value)) - floor) / (100 - floor);
}

// ---- Monitor selection --------------------------------------------------------

/** A monitor as the scope picker sees it: its id and its group (`Ungrouped` when it has none). */
export interface ScopeMonitor {
	id: number;
	group: string;
}

/** The group a monitor counts in: its folder, or `Ungrouped`. */
export function scopeGroup(groupName: string | null | undefined): string {
	const name = (groupName ?? '').trim();
	return name === '' ? UNGROUPED_GROUP : name;
}

/** The monitors a scope keeps (an empty scope keeps them all). Mirrors `Scope::keeps`. */
export function scopeIds(scope: ReportScope, monitors: ScopeMonitor[]): Set<number> {
	const all = scope.targets.length === 0 && scope.groups.length === 0;
	const groups = new Set(scope.groups);
	const targets = new Set(scope.targets);
	return new Set(monitors.filter((mon) => all || targets.has(mon.id) || groups.has(mon.group)).map((mon) => mon.id));
}

/**
 * The scope of a set of ticked monitors, as short as it goes: every monitor
 * is the empty scope (and keeps monitors added later), a whole group is its
 * name (same), the rest are ids. An empty set has no scope: `null`.
 */
export function scopeOf(selected: Set<number>, monitors: ScopeMonitor[]): ReportScope | null {
	const kept = monitors.filter((mon) => selected.has(mon.id));
	if (kept.length === 0) return null;
	if (kept.length === monitors.length) return emptyScope();
	const byGroup = new Map<string, ScopeMonitor[]>();
	for (const mon of monitors) byGroup.set(mon.group, [...(byGroup.get(mon.group) ?? []), mon]);
	const groups: string[] = [];
	const targets: number[] = [];
	for (const [group, members] of byGroup) {
		if (members.every((mon) => selected.has(mon.id))) groups.push(group);
		else targets.push(...members.filter((mon) => selected.has(mon.id)).map((mon) => mon.id));
	}
	return { targets: targets.sort((a, b) => a - b), groups: groups.sort((a, b) => a.localeCompare(b)) };
}
