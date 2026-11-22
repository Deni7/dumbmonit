/**
 * Periodic email reports. Mirrors `crates/server/src/api/reports.rs`.
 * Administrators only, reading included: the list holds recipients' addresses.
 */
import { m } from '#lib/paraglide/messages.js';
import { ApiError, clientViewHeaders, request } from './client';
import { slaParams, type ReportLayout, type ReportScope, type SlaReport, type SlaRequest } from '../sla-report.js';

// The layout, the scope and the report data are plain types and pure helpers,
// kept apart so tests can load them without the network layer.
export * from '../sla-report.js';

export type ReportFrequency = 'daily' | 'weekly' | 'monthly';

/** `digest`: the rolling summary; `sla`: the previous calendar month, with PDF and CSV files. */
export type ReportKind = 'digest' | 'sla';

export interface ReportSchedule {
	id: number;
	name: string;
	enabled: boolean;
	frequency: ReportFrequency;
	/** 0 is Monday. */
	weekday: number;
	/** 1 to 28. */
	day_of_month: number;
	/** 0 to 23, in `timezone`. */
	hour: number;
	/** IANA name. */
	timezone: string;
	recipients: string[];
	/** An `smtp` channel; `null`: the first enabled one. */
	channel_id: number | null;
	/** RFC 3339, UTC. */
	last_sent_at: string | null;
	last_error: string | null;
	next_run_at: string | null;
	kind: ReportKind;
	attach_pdf: boolean;
	attach_csv: boolean;
	/** Status page whose title heads the SLA report; `null`: DumbMonit. */
	page_id: number | null;
	/** Availability objective in percent (SLA report); `null`: none. */
	objective: number | null;
	/** SLA report only. */
	layout: ReportLayout;
	/** SLA report only. */
	scope: ReportScope;
}

/** Body of `POST` and `PUT` (a full replacement). */
export interface ReportSchedulePayload {
	name: string;
	enabled: boolean;
	frequency: ReportFrequency;
	weekday: number;
	day_of_month: number;
	hour: number;
	timezone: string;
	recipients: string[];
	channel_id: number | null;
	kind: ReportKind;
	attach_pdf: boolean;
	attach_csv: boolean;
	page_id: number | null;
	objective: number | null;
	layout: ReportLayout;
	scope: ReportScope;
}

export interface ReportPreviewResult {
	sent: number;
	failed: number;
}

/** Recipients allowed on one report. */
export const MAX_REPORT_RECIPIENTS = 20;

export function listReportSchedules(signal?: AbortSignal): Promise<ReportSchedule[]> {
	return request<ReportSchedule[]>('/reports/schedules', { signal });
}

export function createReportSchedule(payload: ReportSchedulePayload): Promise<ReportSchedule> {
	return request<ReportSchedule>('/reports/schedules', { method: 'POST', body: payload });
}

export function updateReportSchedule(id: number, payload: ReportSchedulePayload): Promise<ReportSchedule> {
	return request<ReportSchedule>(`/reports/schedules/${id}`, { method: 'PUT', body: payload });
}

export function deleteReportSchedule(id: number): Promise<void> {
	return request<void>(`/reports/schedules/${id}`, { method: 'DELETE' });
}

/** Emails the current report, marked as a preview, to the saved recipients. */
export function sendReportPreview(id: number): Promise<ReportPreviewResult> {
	return request<ReportPreviewResult>(`/reports/schedules/${id}/send-test`, { method: 'POST' });
}

/** Builds the report as data (same rules and access as the PDF). */
export function getSlaReport(request_: SlaRequest, signal?: AbortSignal): Promise<SlaReport> {
	const params = slaParams({ ...request_, format: 'json' });
	return request<SlaReport>(`/reports/sla?${params}`, { signal });
}

/** Reads a file from the report API: the blob and the name the server gave it. */
async function fetchReportFile(path: string, fallbackName: string): Promise<{ blob: Blob; filename: string }> {
	let response: Response;
	try {
		response = await fetch(`/api${path}`, {
			headers: { 'x-requested-with': 'DumbMonit', ...clientViewHeaders() }
		});
	} catch {
		throw new ApiError(m.api_error_unreachable(), 0);
	}
	if (!response.ok) {
		let message = m.api_error_status({ status: response.status });
		try {
			const body: unknown = await response.json();
			if (body && typeof body === 'object' && 'error' in body && typeof body.error === 'string') message = body.error;
		} catch {
			// Not JSON: keep the generic message.
		}
		throw new ApiError(message, response.status);
	}
	const disposition = response.headers.get('content-disposition') ?? '';
	const filename = /filename="([^"]+)"/.exec(disposition)?.[1] ?? fallbackName;
	return { blob: await response.blob(), filename };
}

/**
 * Builds the availability report of a chosen period and returns the file.
 * Administrators only. A refusal comes back as an `ApiError` like any other call.
 */
export function downloadSlaReport(request_: SlaRequest): Promise<{ blob: Blob; filename: string }> {
	const params = slaParams(request_);
	return fetchReportFile(`/reports/sla?${params}`, `availability.${request_.format === 'pdf' ? 'pdf' : 'csv'}`);
}

/** Retries a report whose last send failed: a real send to the saved recipients. */
export function retryReportSend(id: number): Promise<ReportPreviewResult> {
	return request<ReportPreviewResult>(`/reports/schedules/${id}/retry`, { method: 'POST' });
}

/**
 * The PDF of the last SLA report sent, made again for the same period (the
 * server keeps no attachments). `inline` marks it to be shown, not saved.
 */
export function downloadScheduledReportPdf(id: number, inline = true): Promise<{ blob: Blob; filename: string }> {
	return fetchReportFile(`/reports/schedules/${id}/pdf${inline ? '?disposition=inline' : ''}`, 'availability.pdf');
}

/** Logo uploaded for SLA reports (layouts set to `custom`): PNG or JPEG, as a base64 or `data:` URL. */
export function uploadReportLogo(data: string): Promise<void> {
	return request<void>('/reports/logo', { method: 'PUT', body: { data } });
}

export function deleteReportLogo(): Promise<void> {
	return request<void>('/reports/logo', { method: 'DELETE' });
}

/** Image URL of the report logo; `version` busts the browser cache after a change. */
export function reportLogoUrl(version = ''): string {
	return `/api/reports/logo${version ? `?v=${encodeURIComponent(version)}` : ''}`;
}
