/**
 * Two-factor authentication (TOTP) of the current account, the admin reset of
 * another account's second factor, and the security audit log.
 */
import { request } from './client';
import type { AuditEntry, TotpEnrolment, TotpStatus } from './types';

export function getTotpStatus(signal?: AbortSignal): Promise<TotpStatus> {
	return request<TotpStatus>('/auth/totp', { signal });
}

/** Proposes a new secret. A 401 means "wrong password", never "session expired". */
export function enrolTotp(password: string): Promise<TotpEnrolment> {
	return request<TotpEnrolment>('/auth/totp/enroll', {
		method: 'POST',
		body: { password },
		allowUnauthorized: true
	});
}

/** Confirms the enrolment with a first code; returns the recovery codes, shown once. */
export async function verifyTotp(code: string): Promise<string[]> {
	const reply = await request<{ recovery_codes: string[] }>('/auth/totp/verify', {
		method: 'POST',
		body: { code },
		allowUnauthorized: true
	});
	return reply.recovery_codes;
}

/**
 * Replaces the recovery codes without turning two-factor off. Confirmed by a
 * current code from the app, or by the password; old codes stop working.
 * A 401 means "wrong code or password", never "session expired".
 */
export async function regenerateRecoveryCodes(confirm: { code?: string; password?: string }): Promise<string[]> {
	const reply = await request<{ recovery_codes: string[] }>('/auth/totp/recovery-codes', {
		method: 'POST',
		body: confirm,
		allowUnauthorized: true
	});
	return reply.recovery_codes;
}

export function disableTotp(password: string): Promise<void> {
	return request<void>('/auth/totp', { method: 'DELETE', body: { password }, allowUnauthorized: true });
}

/** Admin: removes the second factor of another account (lost device). */
export function resetUserTotp(id: number): Promise<void> {
	return request<void>(`/users/${id}/totp`, { method: 'DELETE' });
}

/** Filters of the audit log; every field is optional and they combine with "and". */
export interface AuditFilter {
	/** Part of the account name. */
	actor?: string;
	/** Exact event, for example `login.failed`. */
	action?: string;
	/** Exact client address. */
	ip?: string;
	/** `YYYY-MM-DD` (UTC), oldest day included. */
	since?: string;
	/** `YYYY-MM-DD` (UTC), newest day included. */
	until?: string;
}

function auditParams(filter: AuditFilter): Record<string, string> {
	const params: Record<string, string> = {};
	for (const [key, value] of Object.entries(filter)) {
		if (typeof value === 'string' && value.trim() !== '') params[key] = value.trim();
	}
	return params;
}

/** One page of the audit log, newest first. `before` is the id of the last entry already read. */
export function listAuditLog(
	filter: AuditFilter = {},
	options: { limit?: number; before?: number; signal?: AbortSignal } = {}
): Promise<AuditEntry[]> {
	const query: Record<string, string | number> = { ...auditParams(filter), limit: options.limit ?? 100 };
	if (options.before !== undefined) query.before = options.before;
	return request<AuditEntry[]>('/auth/audit', { query, signal: options.signal });
}

/** Download URL of the filtered log (administrator session): a link, not a fetch. */
export function auditExportUrl(format: 'csv' | 'json', filter: AuditFilter = {}): string {
	const params = new URLSearchParams({ format, ...auditParams(filter) });
	return `/api/auth/audit/export?${params}`;
}
