import type { TeamGrant } from './teams-clients.js';

// --- Authentication ---------------------------------------------------------

/**
 * `operator` handles alerts (ack, ignore, snooze) but changes no configuration.
 * `wall` is a wall display: it signs in straight to `/wall` and reaches nothing else.
 */
export type Role = 'admin' | 'operator' | 'viewer' | 'wall';

/** How an account signs in: a local password, the LDAP / AD directory, or the identity provider. */
export type AuthMethod = 'password' | 'ldap' | 'oidc' | 'saml';

/** An account, as returned by `/api/users` and `/api/auth/me`. Never carries a hash. */
export interface User {
	id: number;
	username: string;
	display_name: string;
	role: Role;
	auth: AuthMethod;
	disabled: boolean;
	/** Two-factor authentication is active on this account. */
	totp_enabled: boolean;
	created_at: string;
	last_login_at: string | null;
	/** Optional contact address; never used to sign in. */
	email?: string | null;
	/** What the account may reach, when the server tells (`/api/auth/me`). */
	access?: AccessInfo;
	/** Custom role of the account; its `role` is then `viewer`. */
	custom_role?: CustomRoleRef | null;
	/** What the account may do, in the terms of the permission catalog. */
	permissions?: Permission[];
}

/**
 * A permission of the server's fixed catalog (`GET /api/roles/permissions`).
 * The known names are listed; a newer server may send others, shown by their label.
 */
export type Permission =
	| 'view'
	| 'alerts.operate'
	| 'monitors.manage'
	| 'notifications.manage'
	| 'status_pages.manage'
	| 'maintenance.manage'
	| 'reports.view'
	| (string & {});

/** The custom role of an account, by id and name. */
export interface CustomRoleRef {
	id: number;
	name: string;
}

/** One entry of the permission catalog. */
export interface PermissionInfo {
	id: Permission;
	/** English name, for a permission the interface does not translate. */
	label: string;
	description: string;
	/** Always part of a custom role (`view`). */
	implied: boolean;
	/** API routes the permission opens, beyond what a viewer reads (`*` = one segment). */
	routes: { method: string; path: string }[];
}

/** A custom role, as returned by `/api/roles`. */
export interface CustomRole {
	id: number;
	name: string;
	description: string;
	permissions: Permission[];
	/** Accounts that have the role; a role in use cannot be deleted. */
	users: number;
	created_at: string;
	updated_at: string;
}

export interface CustomRoleInput {
	name: string;
	description?: string;
	permissions: Permission[];
}

/** Team rights: `read` < `operate` < `manage`. */
export type GrantLevel = 'read' | 'operate' | 'manage';

/** `access` of `GET /api/auth/me`. `scoped` accounts only see the devices their teams grant. */
export interface AccessInfo {
	scoped: boolean;
	teams: string[];
	grants: TeamGrant[];
	/** What the global role caps rights at (viewer: read, operator: manage); `null` for admins. */
	ceiling: GrantLevel | null;
}

/** What the sign-in screen needs to know about single sign-on, without a session. */
export interface OidcStatus {
	enabled: boolean;
	/** Button label ("Continue with …"). */
	provider_name: string;
	/** Plain navigation target, not an API call. */
	login_url: string;
}

/** SAML sign-in availability, read without a session for the sign-in screen. */
export interface SamlStatus {
	enabled: boolean;
	label: string;
}

/** What the sign-in screen needs to know about the directory, without a session. */
export interface LdapStatus {
	enabled: boolean;
}

/** Raw response of `GET /api/auth/status`. */
export interface AuthStatus {
	/** True as soon as an account exists on the instance. */
	configured: boolean;
	/** True if the current session is valid. */
	authenticated: boolean;
	/** The signed-in account, when `authenticated`. */
	user?: User;
	oidc?: OidcStatus;
	/** LDAP / Active Directory sign-in: usernames without a local password go to the directory. */
	ldap?: LdapStatus;
	saml?: SamlStatus;
	/** At least one passkey is registered: the sign-in screen offers to use one. */
	passkeys?: boolean;
	/** Public demo mode: sign in with demo / demo; every change answers 403 `{ demo: true }`. */
	demo: boolean;
	/** HTTPS on the web port, for the "session in clear" banner. */
	https?: HttpsStatus;
	/** The instance policy holds this session at two-factor enrolment. */
	totp_setup_required?: boolean;
	/** "Forgot password?" emails a reset link; otherwise it points to the docs. */
	password_reset?: boolean;
}

/** `AuthStatus.https`: nothing secret, the fingerprint is what every client receives. */
export interface HttpsStatus {
	/** The web port also serves HTTPS. */
	available: boolean;
	/** This request came over HTTPS (direct, or through a trusted proxy). */
	secure: boolean;
	fingerprint_sha256: string | null;
	/** The certificate is self-signed: browsers warn once. */
	self_signed: boolean;
}

// --- Two-factor authentication (TOTP) and audit log ---------------------------

/** Outcome of `POST /auth/login`: either the session is open, or a second step is needed. */
export interface LoginOutcome {
	totp_required: boolean;
	/** Short-lived token to present with the code on `/auth/login/totp`. */
	pending: string | null;
}

/** Mirrors `api::totp::TotpStatus`. */
export interface TotpStatus {
	enabled: boolean;
	pending: boolean;
	recovery_codes_left: number;
}

/** Mirrors `api::totp::Enrolment`. */
export interface TotpEnrolment {
	secret: string;
	otpauth_uri: string;
	issuer: string;
	account: string;
}

/** How a session was opened. Mirrors `auth::session::Via`. */
export type SessionVia = 'password' | 'passkey' | 'ldap' | 'oidc' | 'saml';

/** One open session of the current account. Mirrors `auth::session::SessionView`. */
export interface Session {
	/** Public part of the session cookie: it names the session, it cannot open it. */
	id: string;
	created_at: string;
	/** Updated at most every five minutes. */
	last_seen_at: string | null;
	expires_at: string;
	ip: string | null;
	user_agent: string | null;
	browser: string | null;
	os: string | null;
	via: SessionVia | null;
	/** The session making the request: it is closed by signing out. */
	current: boolean;
}

/** Mirrors `auth::policy::Policy`. */
export interface AuthPolicy {
	/** Admins and operators signing in with a local password must use two-factor. */
	require_totp: boolean;
}

/** Mirrors `auth::audit::Entry`. */
export interface AuditEntry {
	id: number;
	at: string;
	actor: string | null;
	action: string;
	subject: string | null;
	ip: string | null;
}
