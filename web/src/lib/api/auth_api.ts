import { request, ApiError } from './client';
import type {
	AuthState,
	AuthStatus,
	CreateUserPayload,
	LdapConfig,
	LdapConfigPayload,
	LdapTestReport,
	LoginOutcome,
	OidcConfig,
	OidcConfigPayload,
	OidcTestReport,
	UpdateUserPayload,
	User
} from './types';

// --- Authentication ---------------------------------------------------------

const NO_OIDC = { enabled: false, provider_name: 'SSO', login_url: '/api/auth/oidc/start' };

/**
 * Protection state of the instance.
 *
 * As long as authentication is not deployed on the server, the route answers
 * 404: the instance is then declared unprotected (`available: false`) and the
 * interface works normally, rather than locking itself entirely.
 */
export async function getAuthStatus(signal?: AbortSignal): Promise<AuthState> {
	try {
		const status = await request<AuthStatus>('/auth/status', {
			signal,
			anticipated: true,
			allowUnauthorized: true
		});
		return {
			available: true,
			configured: Boolean(status?.configured),
			authenticated: Boolean(status?.authenticated),
			user: status?.user ?? null,
			oidc: status?.oidc ?? NO_OIDC,
			ldap: { enabled: Boolean(status?.ldap?.enabled) },
			saml: { enabled: Boolean(status?.saml?.enabled), label: status?.saml?.label || 'SAML' },
			passkeys: Boolean(status?.passkeys),
			demo: Boolean(status?.demo),
			https: status?.https ?? null,
			totpSetupRequired: status?.totp_setup_required === true,
			passwordReset: status?.password_reset === true
		};
	} catch (cause) {
		if (cause instanceof DOMException && cause.name === 'AbortError') throw cause;
		if (cause instanceof ApiError && cause.missing) {
			return { available: false, configured: false, authenticated: true, user: null, oidc: NO_OIDC, ldap: { enabled: false }, saml: { enabled: false, label: 'SAML' }, passkeys: false, demo: false, https: null, totpSetupRequired: false };
		}
		throw cause;
	}
}

/**
 * Creates the first admin account. `setupCode` is the one-time code the server
 * prints in its logs at startup. Answers 409 if an admin already exists, 401
 * for a wrong code, 429 after too many wrong codes.
 */
export function setupAccount(setupCode: string, username: string, password: string): Promise<void> {
	return request<void>('/auth/setup', {
		method: 'POST',
		body: { setup_code: setupCode, username, password },
		allowUnauthorized: true
	});
}

/**
 * Opens a session. The cookie set by the server is `HttpOnly`: the interface
 * never reads it. When the account has two-factor authentication, the server
 * answers `200 { totp_required: true, pending }` instead of `204`: the session
 * is only opened by `loginTotp`.
 */
export async function login(username: string, password: string): Promise<LoginOutcome> {
	const reply = await request<{ totp_required?: boolean; pending?: string } | undefined>('/auth/login', {
		method: 'POST',
		body: { username, password },
		allowUnauthorized: true
	});
	return { totp_required: Boolean(reply?.totp_required), pending: reply?.pending ?? null };
}

/** Second step of the sign-in: a six-digit code or a recovery code. */
export function loginTotp(pending: string, code: string): Promise<void> {
	return request<void>('/auth/login/totp', {
		method: 'POST',
		body: { pending, code },
		allowUnauthorized: true
	});
}

/** Closes the current session. */
export function logout(): Promise<void> {
	return request<void>('/auth/logout', { method: 'POST', allowUnauthorized: true });
}

/**
 * Changes the current account's password.
 *
 * A 401 here means "wrong current password", not "expired session": it must
 * never send the user back to the sign-in screen.
 */
export function changePassword(currentPassword: string, newPassword: string): Promise<void> {
	return request<void>('/auth/password', {
		method: 'POST',
		body: { current_password: currentPassword, new_password: newPassword },
		allowUnauthorized: true
	});
}

// --- Users (admin only) -----------------------------------------------------

export function listUsers(signal?: AbortSignal): Promise<User[]> {
	return request<User[]>('/users', { signal });
}

export function createUser(payload: CreateUserPayload): Promise<User> {
	return request<User>('/users', { method: 'POST', body: payload });
}

export function updateUser(id: number, payload: UpdateUserPayload): Promise<User> {
	return request<User>(`/users/${id}`, { method: 'PUT', body: payload });
}

export function deleteUser(id: number): Promise<void> {
	return request<void>(`/users/${id}`, { method: 'DELETE' });
}

// --- Single sign-on (admin only) --------------------------------------------

export function getOidcConfig(signal?: AbortSignal): Promise<OidcConfig> {
	return request<OidcConfig>('/auth/oidc/config', { signal });
}

export function saveOidcConfig(payload: OidcConfigPayload): Promise<OidcConfig> {
	return request<OidcConfig>('/auth/oidc/config', { method: 'PUT', body: payload });
}

/** Forgets the saved settings: environment variables apply again, or SSO turns off. */
export function clearOidcConfig(): Promise<OidcConfig> {
	return request<OidcConfig>('/auth/oidc/config', { method: 'DELETE' });
}

/** Runs discovery against `issuer` (or the saved one) and reports the endpoints found. */
export function testOidcDiscovery(issuer?: string): Promise<OidcTestReport> {
	return request<OidcTestReport>('/auth/oidc/test', { method: 'POST', body: { issuer } });
}

// --- LDAP / Active Directory (admin only) ------------------------------------

export function getLdapConfig(signal?: AbortSignal): Promise<LdapConfig> {
	return request<LdapConfig>('/auth/ldap/config', { signal });
}

export function saveLdapConfig(payload: LdapConfigPayload): Promise<LdapConfig> {
	return request<LdapConfig>('/auth/ldap/config', { method: 'PUT', body: payload });
}

/** Forgets the saved settings: environment variables apply again, or LDAP turns off. */
export function clearLdapConfig(): Promise<LdapConfig> {
	return request<LdapConfig>('/auth/ldap/config', { method: 'DELETE' });
}

/**
 * Connects, binds the service account and, with `username`, looks that user up.
 * `config` is the form as typed; the stored password is only reused by the
 * server for the URL and bind DN it was saved for.
 */
export function testLdapConnection(config?: LdapConfigPayload, username?: string): Promise<LdapTestReport> {
	return request<LdapTestReport>('/auth/ldap/test', { method: 'POST', body: { config, username } });
}
