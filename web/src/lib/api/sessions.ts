/**
 * Open sessions of the current account, signing an account out everywhere
 * (admin), and the instance authentication policy (admin). Mirrors
 * `crates/server/src/api/sessions.rs`.
 */
import { request } from './client';
import type { AuthPolicy, Session } from './types';

export function listSessions(signal?: AbortSignal): Promise<Session[]> {
	return request<Session[]>('/auth/sessions', { signal });
}

/** Closes one of the current account's other sessions (409 for the current one). */
export function revokeSession(id: string): Promise<void> {
	return request<void>(`/auth/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Signs out every other session of the current account; returns how many were closed. */
export async function revokeOtherSessions(): Promise<number> {
	const reply = await request<{ closed: number }>('/auth/sessions/revoke-others', { method: 'POST' });
	return reply.closed;
}

/** Admin: signs an account out everywhere (for one's own account, this session stays). */
export async function signOutUser(id: number): Promise<number> {
	const reply = await request<{ closed: number }>(`/users/${id}/sessions`, { method: 'DELETE' });
	return reply.closed;
}

export function getAuthPolicy(signal?: AbortSignal): Promise<AuthPolicy> {
	return request<AuthPolicy>('/auth/policy', { signal });
}

export function saveAuthPolicy(policy: AuthPolicy): Promise<AuthPolicy> {
	return request<AuthPolicy>('/auth/policy', { method: 'PUT', body: policy });
}
