/** Custom roles and the permission catalog. Admin routes, session only. */
import { request } from './client';
import type { CustomRole, CustomRoleInput, PermissionInfo } from './types';

export function listPermissions(signal?: AbortSignal): Promise<PermissionInfo[]> {
	return request<PermissionInfo[]>('/roles/permissions', { signal });
}

export function listCustomRoles(signal?: AbortSignal): Promise<CustomRole[]> {
	return request<CustomRole[]>('/roles', { signal });
}

export function createCustomRole(payload: CustomRoleInput): Promise<CustomRole> {
	return request<CustomRole>('/roles', { method: 'POST', body: payload });
}

export function updateCustomRole(id: number, payload: CustomRoleInput): Promise<CustomRole> {
	return request<CustomRole>(`/roles/${id}`, { method: 'PUT', body: payload });
}

/** Refused (409) while an account still has the role. */
export function deleteCustomRole(id: number): Promise<void> {
	return request<void>(`/roles/${id}`, { method: 'DELETE' });
}
