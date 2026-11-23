import { request, ApiError } from './client';
import { sortDismissed } from '../alerts-dismiss';
import type {
	AckPayload,
	Alert,
	AlertHistoryEntry,
	AlertRule,
	AlertRulePayload,
	Silence,
	SilenceOccurrence,
	SilencePayload
} from './types';

// --- Alerts (anticipated) ---------------------------------------------------

/** Returns the active alerts, or an empty array if the route does not exist yet. */
export async function listAlerts(signal?: AbortSignal): Promise<Alert[]> {
	try {
		const alerts = await request<Alert[]>('/alerts', { signal, anticipated: true });
		return Array.isArray(alerts) ? alerts : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

/**
 * Recent phase transitions, newest first. `limit` caps the count; `since` is an
 * RFC 3339 lower bound (the server defaults to the last seven days).
 */
export async function listAlertHistory(
	options: { limit?: number; since?: string; dismissed?: boolean } = {},
	signal?: AbortSignal
): Promise<AlertHistoryEntry[]> {
	try {
		const entries = await request<AlertHistoryEntry[]>('/alerts/history', {
			query: { limit: options.limit, since: options.since, dismissed: options.dismissed },
			signal,
			anticipated: true
		});
		return Array.isArray(entries) ? entries : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

/** Clears one resolved history entry from the default view; the row is kept. */
export function dismissAlertHistoryEntry(id: number): Promise<void> {
	return request<void>(`/alerts/history/${id}/dismiss`, { method: 'POST' });
}

/** Clears every resolved history entry at once; returns how many were cleared. */
export function dismissResolvedAlertHistory(): Promise<{ dismissed: number }> {
	return request<{ dismissed: number }>('/alerts/history/dismiss-resolved', { method: 'POST' });
}

/** Lists the alert rules, or an empty array if the route does not exist yet. */
export async function listAlertRules(signal?: AbortSignal): Promise<AlertRule[]> {
	try {
		const rules = await request<AlertRule[]>('/alerts/rules', { signal, anticipated: true });
		return Array.isArray(rules) ? rules : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

export function createAlertRule(payload: AlertRulePayload): Promise<AlertRule> {
	return request<AlertRule>('/alerts/rules', { method: 'POST', body: payload });
}

export function updateAlertRule(id: number, payload: AlertRulePayload): Promise<AlertRule> {
	return request<AlertRule>(`/alerts/rules/${id}`, { method: 'PUT', body: payload });
}

export function deleteAlertRule(id: number): Promise<void> {
	return request<void>(`/alerts/rules/${id}`, { method: 'DELETE' });
}

export function setAlertRuleEnabled(id: number, enabled: boolean): Promise<AlertRule> {
	return request<AlertRule>(`/alerts/rules/${id}/enable`, {
		method: 'POST',
		body: { enabled }
	});
}

/** Resets a built-in rule to the values it ships with. */
export function resetAlertRule(id: number): Promise<AlertRule> {
	return request<AlertRule>(`/alerts/rules/${id}/reset`, { method: 'POST' });
}

/** Lists the maintenance windows, or an empty array if the route does not exist yet. */
export async function listSilences(signal?: AbortSignal): Promise<Silence[]> {
	try {
		const silences = await request<Silence[]>('/alerts/silences', { signal, anticipated: true });
		return Array.isArray(silences) ? silences : [];
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

/** Windows unrolled into dated occurrences between two instants (ISO, UTC). */
export function listSilenceOccurrences(
	from: string,
	to: string,
	signal?: AbortSignal
): Promise<SilenceOccurrence[]> {
	const query = new URLSearchParams({ from, to });
	return request<SilenceOccurrence[]>(`/alerts/silences/occurrences?${query}`, { signal });
}

export function createSilence(payload: SilencePayload): Promise<Silence> {
	return request<Silence>('/alerts/silences', { method: 'POST', body: payload });
}

/** Edits a maintenance window (also how "end now" and "extend" are done). */
export function updateSilence(id: number, payload: SilencePayload): Promise<Silence> {
	return request<Silence>(`/alerts/silences/${id}`, { method: 'PUT', body: payload });
}

export function deleteSilence(id: number): Promise<void> {
	return request<void>(`/alerts/silences/${id}`, { method: 'DELETE' });
}

/** Acknowledges an alert ("I know, stop reminding me"); returns the updated alert. */
export function ackAlert(fingerprint: string, payload: AckPayload = {}): Promise<Alert> {
	return request<Alert>(`/alerts/${encodeURIComponent(fingerprint)}/ack`, {
		method: 'POST',
		body: payload
	});
}

/** Lifts an acknowledgement; returns the updated alert. */
export function unackAlert(fingerprint: string): Promise<Alert> {
	return request<Alert>(`/alerts/${encodeURIComponent(fingerprint)}/ack`, { method: 'DELETE' });
}

/** Dismisses an alert: it leaves every list and count for good (this occurrence); returns the alert. */
export function dismissAlert(fingerprint: string): Promise<Alert> {
	return request<Alert>(`/alerts/${encodeURIComponent(fingerprint)}/dismiss`, { method: 'POST' });
}

/** Undoes a dismissal; returns the alert, back in the active list. */
export function undismissAlert(fingerprint: string): Promise<Alert> {
	return request<Alert>(`/alerts/${encodeURIComponent(fingerprint)}/dismiss`, { method: 'DELETE' });
}

/** The dismissed alerts that are still active (`GET /alerts?dismissed=true`), newest first. */
export async function listDismissedAlerts(signal?: AbortSignal): Promise<Alert[]> {
	const alerts = await request<Alert[]>('/alerts', { signal, query: { dismissed: true } });
	return Array.isArray(alerts) ? sortDismissed(alerts) : [];
}
