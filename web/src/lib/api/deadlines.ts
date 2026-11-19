/**
 * Deadlines across the instance. Mirrors `crates/server/src/api/deadlines.rs`:
 * read from the last point of each collected expiry series, nothing is probed.
 */
import { request } from './client';
import type { Deadline, TargetId } from './types';

export function listDeadlines(
	options: { target?: TargetId; maxDays?: number; upcoming?: boolean; signal?: AbortSignal } = {}
): Promise<Deadline[]> {
	const params = new URLSearchParams();
	if (options.target !== undefined) params.set('target', String(options.target));
	if (options.maxDays !== undefined) params.set('max_days', String(options.maxDays));
	if (options.upcoming) params.set('upcoming', 'true');
	const query = params.toString();
	return request<Deadline[]>(`/deadlines${query ? `?${query}` : ''}`, { signal: options.signal });
}
