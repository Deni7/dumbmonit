/**
 * Names and help of the permissions the interface knows, translated. A
 * permission a newer server sends is shown with the English label and
 * description it comes with.
 */
import type { PermissionInfo } from '#lib/api/types.js';
import { m } from '#lib/paraglide/messages.js';

const LABELS: Record<string, () => string> = {
	view: () => m.w6roles_perm_view_label(),
	'alerts.operate': () => m.w6roles_perm_alerts_label(),
	'monitors.manage': () => m.w6roles_perm_monitors_label(),
	'notifications.manage': () => m.w6roles_perm_notifications_label(),
	'status_pages.manage': () => m.w6roles_perm_status_pages_label(),
	'maintenance.manage': () => m.w6roles_perm_maintenance_label(),
	'reports.view': () => m.w6roles_perm_reports_label()
};

const HELP: Record<string, () => string> = {
	view: () => m.w6roles_perm_view_help(),
	'alerts.operate': () => m.w6roles_perm_alerts_help(),
	'monitors.manage': () => m.w6roles_perm_monitors_help(),
	'notifications.manage': () => m.w6roles_perm_notifications_help(),
	'status_pages.manage': () => m.w6roles_perm_status_pages_help(),
	'maintenance.manage': () => m.w6roles_perm_maintenance_help(),
	'reports.view': () => m.w6roles_perm_reports_help()
};

/** Short name of a permission, from its id (and the catalog entry, if known). */
export function permissionLabel(id: string, info?: PermissionInfo): string {
	return LABELS[id]?.() ?? info?.label ?? id;
}

/** One sentence on what the permission opens. */
export function permissionHelp(info: PermissionInfo): string {
	return HELP[info.id]?.() ?? info.description;
}
