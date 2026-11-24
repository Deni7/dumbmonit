import { request } from './client';
import type {
	AlertLinkResult,
	AlertLinkView,
	Incident,
	IncidentPayload,
	IncidentUpdatePayload,
	PostmortemPayload,
	PublicDayDetail,
	PublicOlderIncidents,
	PublicStatus,
	StatusPage,
	StatusPageAccess,
	StatusPageItem,
	StatusPageItemPayload,
	StatusPageLanguages,
	StatusPagePayload,
	StatusSubscribePayload,
	StatusSubscribeReply,
	StatusSubscriber,
	StatusSubscriptionSettings
} from './types';

// --- Status pages -----------------------------------------------------------

export function listStatusPages(signal?: AbortSignal): Promise<StatusPage[]> {
	return request<StatusPage[]>('/status-pages', { signal });
}

export function getStatusPage(id: number, signal?: AbortSignal): Promise<StatusPage> {
	return request<StatusPage>(`/status-pages/${id}`, { signal });
}

/** The saved page as visitors would read it, published or not, with each service's monitor id (editor preview). */
export function previewStatusPage(id: number, signal?: AbortSignal): Promise<PublicStatus> {
	return request<PublicStatus>(`/status-pages/${id}/preview`, { signal });
}

export function createStatusPage(payload: StatusPagePayload): Promise<StatusPage> {
	return request<StatusPage>('/status-pages', { method: 'POST', body: payload });
}

export function updateStatusPage(id: number, payload: StatusPagePayload): Promise<StatusPage> {
	return request<StatusPage>(`/status-pages/${id}`, { method: 'PUT', body: payload });
}

export function deleteStatusPage(id: number): Promise<void> {
	return request<void>(`/status-pages/${id}`, { method: 'DELETE' });
}

/** Replaces the services of a page; the array order is the display order. */
export function setStatusPageItems(
	id: number,
	items: StatusPageItemPayload[]
): Promise<StatusPageItem[]> {
	return request<StatusPageItem[]>(`/status-pages/${id}/items`, { method: 'PUT', body: items });
}

/** Default language and per-language texts of a status page. */
export function getStatusPageLanguages(id: number, signal?: AbortSignal): Promise<StatusPageLanguages> {
	return request<StatusPageLanguages>(`/status-pages/${id}/languages`, { signal });
}

export function putStatusPageLanguages(id: number, languages: StatusPageLanguages): Promise<StatusPageLanguages> {
	return request<StatusPageLanguages>(`/status-pages/${id}/languages`, { method: 'PUT', body: languages });
}

export function listIncidents(signal?: AbortSignal): Promise<Incident[]> {
	return request<Incident[]>('/incidents', { signal });
}

export function createIncident(payload: IncidentPayload): Promise<Incident> {
	return request<Incident>('/incidents', { method: 'POST', body: payload });
}

export function updateIncident(id: number, payload: IncidentPayload): Promise<Incident> {
	return request<Incident>(`/incidents/${id}`, { method: 'PUT', body: payload });
}

export function deleteIncident(id: number): Promise<void> {
	return request<void>(`/incidents/${id}`, { method: 'DELETE' });
}

/** Posts a message and moves the incident to `status` (or keeps it). */
export function addIncidentUpdate(id: number, payload: IncidentUpdatePayload): Promise<Incident> {
	return request<Incident>(`/incidents/${id}/updates`, { method: 'POST', body: payload });
}

/** Writes or replaces the post-mortem; returns the whole incident. */
export function saveIncidentPostmortem(id: number, payload: PostmortemPayload): Promise<Incident> {
	return request<Incident>(`/incidents/${id}/postmortem`, { method: 'PUT', body: payload });
}

export function deleteIncidentPostmortem(id: number): Promise<void> {
	return request<void>(`/incidents/${id}/postmortem`, { method: 'DELETE' });
}

/** Uploads a logo (PNG, JPEG or WebP, as a base64 or `data:` URL). */
export function uploadStatusPageLogo(id: number, data: string): Promise<void> {
	return request<void>(`/status-pages/${id}/logo`, { method: 'PUT', body: { data } });
}

export function deleteStatusPageLogo(id: number): Promise<void> {
	return request<void>(`/status-pages/${id}/logo`, { method: 'DELETE' });
}

/** Admin preview URL of a page's logo (works for drafts too). */
export function statusPageLogoUrl(id: number, version: string): string {
	return `/api/status-pages/${id}/logo?v=${encodeURIComponent(version)}`;
}

export function listStatusSubscribers(id: number, signal?: AbortSignal): Promise<StatusSubscriber[]> {
	return request<StatusSubscriber[]>(`/status-pages/${id}/subscribers`, { signal });
}

export function deleteStatusSubscriber(id: number, subscriber: number): Promise<void> {
	return request<void>(`/status-pages/${id}/subscribers/${subscriber}`, { method: 'DELETE' });
}

/** Public: asks for a confirmation email. Same answer whatever the address's state. */
export function subscribeToStatus(slug: string, payload: StatusSubscribePayload): Promise<StatusSubscribeReply> {
	return request(`/public/status/${encodeURIComponent(slug)}/subscribe`, { method: 'POST', body: payload });
}

/** Public: confirms an SMS subscription with the code that was texted. */
export function confirmStatusSmsCode(slug: string, phone: string, code: string): Promise<{ status: string }> {
	return request(`/public/status/${encodeURIComponent(slug)}/confirm-code`, { method: 'POST', body: { phone, code } });
}

export function getStatusSubscriptionSettings(id: number, signal?: AbortSignal): Promise<StatusSubscriptionSettings> {
	return request<StatusSubscriptionSettings>(`/status-pages/${id}/subscription`, { signal });
}

export function saveStatusSubscriptionSettings(id: number, settings: StatusSubscriptionSettings): Promise<StatusSubscriptionSettings> {
	return request<StatusSubscriptionSettings>(`/status-pages/${id}/subscription`, { method: 'PUT', body: settings });
}

/** Public: confirms a subscription with the token from the email. */
/**
 * Public: what a notification action link would do. The token travels in the body,
 * never in a URL, so it reaches no access log.
 */
export function inspectAlertLink(token: string): Promise<AlertLinkView> {
	return request<AlertLinkView>('/public/alert-action/inspect', { method: 'POST', body: { token }, allowUnauthorized: true });
}

/** Public: runs a notification action link, once. */
export function runAlertLink(token: string): Promise<AlertLinkResult> {
	return request<AlertLinkResult>('/public/alert-action', { method: 'POST', body: { token }, allowUnauthorized: true });
}

export function confirmStatusSubscription(slug: string, token: string): Promise<{ status: string }> {
	return request(`/public/status/${encodeURIComponent(slug)}/confirm`, { method: 'POST', query: { token } });
}

/** Public: removes a subscription with the token from any update email. */
export function unsubscribeFromStatus(slug: string, token: string): Promise<{ status: string }> {
	return request(`/public/status/${encodeURIComponent(slug)}/unsubscribe`, { method: 'POST', query: { token } });
}

/**
 * Public document of a status page: no session, no cookie needed for a public
 * page. A private page answers 401 (`ApiError.access` says what it wants),
 * which must not sign the visitor out of DumbMonit.
 */
export function getPublicStatus(slug: string, signal?: AbortSignal, lang?: string): Promise<PublicStatus> {
	return request<PublicStatus>(`/public/status/${encodeURIComponent(slug)}`, {
		query: lang ? { lang } : undefined,
		signal,
		allowUnauthorized: true
	});
}

/** Trades a private page's password for its access cookie. */
export function unlockStatusPage(slug: string, password: string): Promise<{ status: string }> {
	return request(`/public/status/${encodeURIComponent(slug)}/access`, {
		method: 'POST',
		body: { password },
		allowUnauthorized: true
	});
}

/** Admin: sets who may read a page. `password` is kept when omitted. */
export function setStatusPageAccess(
	id: number,
	body: { mode: StatusPageAccess; password?: string }
): Promise<StatusPage> {
	return request<StatusPage>(`/status-pages/${id}/access`, { method: 'PUT', body });
}

/** The next slice of resolved incidents older than the page's 30 days. */
export function getPublicOlderIncidents(slug: string, before?: string, signal?: AbortSignal): Promise<PublicOlderIncidents> {
	const query = before ? `?before=${encodeURIComponent(before)}` : '';
	return request<PublicOlderIncidents>(`/public/status/${encodeURIComponent(slug)}/incidents${query}`, { signal });
}

export function getPublicStatusDay(slug: string, date: string, signal?: AbortSignal): Promise<PublicDayDetail> {
	return request<PublicDayDetail>(`/public/status/${encodeURIComponent(slug)}/day/${encodeURIComponent(date)}`, { signal });
}
