/**
 * Account links and personal notifications: "Forgot password?", reset links,
 * invitations, and "My notifications". Mirrors `crates/server/src/api/recovery.rs`,
 * `invitations.rs` and `my_notifications.rs`.
 *
 * The public calls (`/auth/password-reset*`, `/auth/invitations/check|accept`)
 * pass `allowUnauthorized`: a 401 there is an answer, not a lost session.
 */
import { request } from './client';
import type { AlertSeverity, QuietHours, Role } from './types';

// --- Reset links -------------------------------------------------------------

/** Always answers the same, whether an account matches or not. */
export function requestPasswordReset(login: string): Promise<{ message: string }> {
	return request<{ message: string }>('/auth/password-reset', {
		method: 'POST',
		body: { login },
		allowUnauthorized: true
	});
}

export interface ResetCheck {
	username: string;
	/** UTC without suffix. */
	expires_at: string;
}

/** 404 for any link that cannot be used. */
export function checkPasswordReset(token: string): Promise<ResetCheck> {
	return request<ResetCheck>('/auth/password-reset/check', {
		method: 'POST',
		body: { token },
		allowUnauthorized: true
	});
}

/** Sets the new password and signs the account out everywhere. */
export function completePasswordReset(token: string, password: string): Promise<void> {
	return request<void>('/auth/password-reset/complete', {
		method: 'POST',
		body: { token, password },
		allowUnauthorized: true
	});
}

/** A link created by an administrator, shown once. */
export interface AccountLink {
	url: string;
	expires_at: string;
	emailed: boolean;
	email_error?: string | null;
}

/** Admin: a reset link for an account, emailed when `send` and possible. */
export function createResetLink(userId: number, send: boolean): Promise<AccountLink> {
	return request<AccountLink>(`/users/${userId}/reset-link`, { method: 'POST', body: { send } });
}

export interface AccountLinkSettings {
	self_service: boolean;
	reset_minutes: number;
	invite_hours: number;
}

export interface AccountLinkSettingsView extends AccountLinkSettings {
	mail_server: boolean;
	link_base: string | null;
	self_service_available: boolean;
}

export function getAccountLinkSettings(signal?: AbortSignal): Promise<AccountLinkSettingsView> {
	return request<AccountLinkSettingsView>('/auth/recovery', { signal });
}

export function saveAccountLinkSettings(settings: AccountLinkSettings): Promise<AccountLinkSettingsView> {
	return request<AccountLinkSettingsView>('/auth/recovery', { method: 'PUT', body: settings });
}

// --- Invitations -------------------------------------------------------------

export type InvitableRole = Exclude<Role, 'wall'>;

export interface Invitation {
	id: number;
	email: string;
	role: InvitableRole;
	teams: { id: number; name: string }[];
	invited_by: string;
	created_at: string;
	expires_at: string;
	last_sent_at: string | null;
	/** No longer accepted; can be resent. */
	expired: boolean;
}

export interface InvitationSent {
	invitation: Invitation;
	/** `/invite?token=…`, shown once. */
	url: string;
	emailed: boolean;
	email_error?: string | null;
}

export interface InvitationPayload {
	email: string;
	role: InvitableRole;
	teams: number[];
	send: boolean;
}

export function listInvitations(signal?: AbortSignal): Promise<Invitation[]> {
	return request<Invitation[]>('/users/invitations', { signal });
}

export function createInvitation(payload: InvitationPayload): Promise<InvitationSent> {
	return request<InvitationSent>('/users/invitations', { method: 'POST', body: payload });
}

export function resendInvitation(id: number, send: boolean): Promise<InvitationSent> {
	return request<InvitationSent>(`/users/invitations/${id}/resend`, { method: 'POST', body: { send } });
}

export function revokeInvitation(id: number): Promise<void> {
	return request<void>(`/users/invitations/${id}`, { method: 'DELETE' });
}

export interface InvitationCheck {
	email: string;
	role: InvitableRole;
	suggested_username: string;
	invited_by: string;
	expires_at: string;
	/** Single sign-on can be used instead of a password. */
	sso: boolean;
}

export function checkInvitation(token: string): Promise<InvitationCheck> {
	return request<InvitationCheck>('/auth/invitations/check', {
		method: 'POST',
		body: { token },
		allowUnauthorized: true
	});
}

export interface InvitationAcceptance {
	token: string;
	username?: string;
	display_name?: string;
	password?: string;
	/** No password: the account waits for its first single sign-on. */
	sso?: boolean;
}

export function acceptInvitation(payload: InvitationAcceptance): Promise<{ username: string }> {
	return request<{ username: string }>('/auth/invitations/accept', {
		method: 'POST',
		body: payload,
		allowUnauthorized: true
	});
}

// --- My notifications ----------------------------------------------------------

export type PersonalScope = 'all' | 'teams' | 'pick';

export interface PersonalPrefs {
	email: boolean;
	webpush: boolean;
	min_severity: AlertSeverity;
	notify_resolved: boolean;
	scope: PersonalScope;
	folders: string[];
	/** `key=value`, or `key` alone. */
	tags: string[];
	/** 0: no minimum delay between two messages for the same alert. */
	min_interval_secs: number;
	quiet_hours: QuietHours | null;
}

export interface MyNotifications {
	prefs: PersonalPrefs;
	/** The account's address, set by an administrator. */
	email: string | null;
	mail_server: boolean;
	push_devices: number;
	teams: string[];
	folders: string[];
	tags: string[];
}

export interface PersonalDelivery {
	ok: boolean;
	error?: string | null;
}

export function getMyNotifications(signal?: AbortSignal): Promise<MyNotifications> {
	return request<MyNotifications>('/auth/notifications', { signal });
}

export function saveMyNotifications(prefs: PersonalPrefs): Promise<MyNotifications> {
	return request<MyNotifications>('/auth/notifications', { method: 'PUT', body: prefs });
}

export interface PersonalTestReport {
	/** `null`: not tried. */
	email: PersonalDelivery | null;
	push: PersonalDelivery | null;
}

export function testMyNotifications(): Promise<PersonalTestReport> {
	return request<PersonalTestReport>('/auth/notifications/test', { method: 'POST' });
}
