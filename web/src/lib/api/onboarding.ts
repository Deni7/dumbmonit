import { request } from './client';
import type {
	AccessInfo,
	MspAccess,
	OnboardingPatch,
	OnboardingState
} from './types';

// --- First-run guide (anticipated) ------------------------------------------

/**
 * Reads the first-run guide's state. Anticipated: a server without the route
 * answers 404, and the caller then treats the guide as finished rather than
 * showing a guide it could never dismiss.
 */
export function getOnboarding(signal?: AbortSignal): Promise<OnboardingState> {
	return request<OnboardingState>('/onboarding', { signal, anticipated: true });
}

/** Dismisses a step, or the whole guide. Returns the state the server kept. */
export function updateOnboarding(patch: OnboardingPatch): Promise<OnboardingState> {
	return request<OnboardingState>('/onboarding', { method: 'PUT', body: patch });
}

/**
 * Team rights and MSP clients of the current account, in one call. Older
 * servers send neither: unrestricted, MSP off. Never throws (but an abort).
 */
export async function getMyRights(signal?: AbortSignal): Promise<{ access: AccessInfo | null; msp: MspAccess | null }> {
	try {
		const me = await request<{ access?: AccessInfo; msp?: MspAccess }>('/auth/me', { signal, anticipated: true });
		return { access: me?.access ?? null, msp: me?.msp ?? null };
	} catch (cause) {
		if (cause instanceof DOMException && cause.name === 'AbortError') throw cause;
		return { access: null, msp: null };
	}
}

// --- Maintenance calendar (iCal) --------------------------------------------

/** Token and path of the private calendar of every maintenance (administrators only). */
export interface MaintenanceCalendar {
	token: string;
	/** `/api/calendar/maintenance.ics?token=…`; add the origin to subscribe. */
	path: string;
}

/** Reads the private calendar's token, creating it on first use. */
export function getMaintenanceCalendar(signal?: AbortSignal): Promise<MaintenanceCalendar> {
	return request<MaintenanceCalendar>('/calendar/maintenance', { signal });
}

/** Replaces the token: the previous link stops working. */
export function rotateMaintenanceCalendar(): Promise<MaintenanceCalendar> {
	return request<MaintenanceCalendar>('/calendar/maintenance/rotate', { method: 'POST' });
}
