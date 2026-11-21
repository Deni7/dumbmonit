/**
 * Service level objectives. Mirrors `crates/server/src/api/slos.rs`.
 * Everyone reads; administrators write (the server's session guard decides).
 */
import { request } from './client';
import type { Slo, SloDetail, SloPayload } from './types';

export function listSlos(signal?: AbortSignal): Promise<Slo[]> {
	return request<Slo[]>('/slos', { signal });
}

export function getSlo(id: number, signal?: AbortSignal): Promise<SloDetail> {
	return request<SloDetail>(`/slos/${id}`, { signal });
}

export function createSlo(payload: SloPayload): Promise<SloDetail> {
	return request<SloDetail>('/slos', { method: 'POST', body: payload });
}

export function updateSlo(id: number, payload: SloPayload): Promise<SloDetail> {
	return request<SloDetail>(`/slos/${id}`, { method: 'PUT', body: payload });
}

export function deleteSlo(id: number): Promise<void> {
	return request<void>(`/slos/${id}`, { method: 'DELETE' });
}
