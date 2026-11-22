/**
 * Change markers (deployments, updates, maintenance) drawn on the charts.
 * Mirrors `crates/server/src/api/annotations.rs`. Everyone reads; administrators,
 * operators and `write` tokens add and remove.
 */
import { request } from './client';
import type { Annotation, AnnotationPayload, TargetId } from './types';

export function listAnnotations(
	options: { from?: Date; to?: Date; target?: TargetId; signal?: AbortSignal } = {}
): Promise<Annotation[]> {
	const params = new URLSearchParams();
	if (options.from) params.set('from', options.from.toISOString());
	if (options.to) params.set('to', options.to.toISOString());
	if (options.target !== undefined) params.set('target', String(options.target));
	const query = params.toString();
	return request<Annotation[]>(`/annotations${query ? `?${query}` : ''}`, { signal: options.signal });
}

export function createAnnotation(payload: AnnotationPayload): Promise<Annotation> {
	return request<Annotation>('/annotations', { method: 'POST', body: payload });
}

export function deleteAnnotation(id: number): Promise<void> {
	return request<void>(`/annotations/${id}`, { method: 'DELETE' });
}
