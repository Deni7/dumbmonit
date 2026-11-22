/**
 * The reports history: what each scheduled report last did, and the filters
 * over it (by type and by result). Pure, so the tests load it directly.
 */
import type { ReportKind, ReportSchedule, ReportSchedulePayload } from '#lib/api/reports.js';

/** What the history says about a report's last send: the word of its plate. */
export type ReportOutcome = 'failed' | 'sent' | 'waiting';

export interface HistoryRow {
	kind: ReportKind;
	last_sent_at: string | null;
	last_error: string | null;
}

export function outcomeOf(row: Pick<HistoryRow, 'last_sent_at' | 'last_error'>): ReportOutcome {
	if (row.last_error) return 'failed';
	return row.last_sent_at ? 'sent' : 'waiting';
}

export const KIND_FILTERS = ['all', 'digest', 'sla'] as const;
export const OUTCOME_FILTERS = ['all', 'sent', 'failed', 'waiting'] as const;
export type KindFilter = (typeof KIND_FILTERS)[number];
export type OutcomeFilter = (typeof OUTCOME_FILTERS)[number];

/** The rows of a type (`all`: every type) with a result (`all`: every result). */
export function filterHistory<T extends HistoryRow>(rows: T[], kind: KindFilter, outcome: OutcomeFilter): T[] {
	return rows.filter((row) => (kind === 'all' || row.kind === kind) && (outcome === 'all' || outcomeOf(row) === outcome));
}

/** A failed report can be sent again. */
export function canRetry(row: Pick<HistoryRow, 'last_error'>): boolean {
	return !!row.last_error;
}

/** Only an SLA report has a PDF, and only once it was sent. */
export function canDownloadPdf(row: Pick<HistoryRow, 'kind' | 'last_sent_at'>): boolean {
	return row.kind === 'sla' && !!row.last_sent_at;
}

/**
 * Starting values of a new report copied from `source` under a new name: the
 * settings, never the history (last send, error, next run) or the id. The
 * layout and the scope are deep copies, so editing the copy leaves the
 * original alone.
 */
export function duplicateDraft(source: ReportSchedule, name: string): ReportSchedulePayload {
	const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
	return {
		name,
		enabled: source.enabled,
		frequency: source.frequency,
		weekday: source.weekday,
		day_of_month: source.day_of_month,
		hour: source.hour,
		timezone: source.timezone,
		recipients: [...source.recipients],
		channel_id: source.channel_id,
		kind: source.kind,
		attach_pdf: source.attach_pdf,
		attach_csv: source.attach_csv,
		page_id: source.page_id,
		objective: source.objective,
		layout: copy(source.layout),
		scope: copy(source.scope)
	};
}
