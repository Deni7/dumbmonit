/**
 * Monitor templates: a type, its options and tags without any secret, an
 * interval and notification channels, to prefill the add form. Mirrors
 * `crates/server/src/api/monitor_templates.rs`. Writes are for administrators.
 */
import { request } from './client';
import type { TargetId } from './types';

export interface MonitorTemplate {
	id: number;
	name: string;
	kind: string;
	interval_secs: number;
	/** Options of the type and free tags; never a secret. */
	tags: Record<string, string>;
	/** Channels of a monitor created from it; empty: those of its alert rules. */
	channels: number[];
	created_at: string;
	updated_at: string;
}

export function listMonitorTemplates(signal?: AbortSignal): Promise<MonitorTemplate[]> {
	return request<MonitorTemplate[]>('/monitor-templates', { signal });
}

export function getMonitorTemplate(id: number, signal?: AbortSignal): Promise<MonitorTemplate> {
	return request<MonitorTemplate>(`/monitor-templates/${id}`, { signal });
}

/** Saves a monitor as a template: the server reads what it stores, never a secret. */
export function createMonitorTemplate(name: string, targetId: TargetId): Promise<MonitorTemplate> {
	return request<MonitorTemplate>('/monitor-templates', { method: 'POST', body: { name, target_id: targetId } });
}

export function renameMonitorTemplate(id: number, name: string): Promise<MonitorTemplate> {
	return request<MonitorTemplate>(`/monitor-templates/${id}`, { method: 'PUT', body: { name } });
}

export function deleteMonitorTemplate(id: number): Promise<void> {
	return request<void>(`/monitor-templates/${id}`, { method: 'DELETE' });
}
