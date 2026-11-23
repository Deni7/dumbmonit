import type { GatewayProblem } from './agents.js';
import type { HttpsStatus, LdapStatus, OidcStatus, Role, SamlStatus, User } from './auth.js';

// --- HTTPS and cleartext report (Settings → Security) -----------------------
// Mirrors `GET /api/security/https` (crates/server/src/api/https.rs).

export interface HttpsCertificate {
	source: 'provided' | 'self_signed';
	/** SHA-256 of the certificate, as browsers show it (AB:CD:…). */
	fingerprint_sha256: string;
	/** `sha256//…`, the public-key pin of `curl --pinnedpubkey` (self-signed only). */
	public_key_pin: string | null;
	subject: string;
	issuer: string;
	not_after: string | null;
	names: string[];
}

/** One place where data still travels in clear, or without certificate checks. */
export interface HttpsFinding {
	area: 'web' | 'storage' | 'notifications' | 'monitors' | 'logs' | 'sign_in' | 'agents' | 'backups' | string;
	code: string;
	severity: 'warning' | 'info';
	/** Name or address of the item; never a secret. */
	subject: string;
	link: string | null;
}

export interface HttpsView {
	/** A certificate is loaded and served: always, the web port only serves HTTPS. */
	available: boolean;
	certificate: HttpsCertificate | null;
	error: string | null;
	trusted_proxies: number;
	hsts: string;
	findings: HttpsFinding[];
}

/** `GET /api/security/ports`: the ports to open, computed from the real configuration. */
export type PortExposure = 'lan' | 'wan_conditional' | 'never';
export type PortStatus = 'listening' | 'off' | 'local';
export interface PortEntry {
	/** Stable id (`web`, `agents`, `syslog_udp`, `syslog_tcp`, `traps`, `*_store`, `relay_gateway`); an unknown one is shown as is. */
	id: string;
	/** Port the process listens on (inside the container, or on the relay machine). */
	port: number;
	/** Port to open on the host (the published one). */
	host_port: number;
	protocol: 'tcp' | 'udp';
	exposure: PortExposure;
	status: PortStatus;
	bind: string;
	/** Relay name for a relay gateway. */
	subject: string | null;
	/** False: the port is on another machine (a relay). */
	on_this_host: boolean;
}
export interface PortsView {
	ports: PortEntry[];
	/** `ports:` block for docker-compose. */
	compose: string;
	/** `ufw allow` rules with a network to replace. */
	ufw: string;
}

/** `POST /api/agent/tls/check`: is the encrypted agent channel reachable where agents will dial? */
export type AgentChannelOutcome = 'ok' | 'dns' | 'refused' | 'timeout' | 'tls' | 'foreign' | 'no_websocket';
export interface AgentChannelCheck {
	enabled: boolean;
	url: string | null;
	host: string | null;
	/** Port agents dial (DUMBMONIT_AGENT_PUBLIC_PORT, else the listen port). */
	port: number | null;
	/** Port inside the container (DUMBMONIT_AGENT_LISTEN). */
	listen_port: number | null;
	/** Absent through a relay (the server cannot see the site's network) or when the channel is off. */
	outcome: AgentChannelOutcome | null;
	relay: {
		relay_id: number;
		relay_name: string;
		host: string;
		port: number;
		listening: boolean;
		/** The relay tests its own gateway (up-to-date agent): wait for `reachable`. */
		self_check?: boolean;
		/** Outcome of its last test at `host:port`; `null`: not tested there yet. */
		reachable?: boolean | null;
		/** What failed, as the relay reports it (English). */
		check_error?: string | null;
		/** When the relay ran that test (Unix milliseconds). */
		checked_at_ms?: number | null;
		/** What keeps the gateway from accepting the site, when the server knows it. */
		problem?: GatewayProblem | null;
		/** Address the relay really bound. */
		listen?: string | null;
		/** Why the relay could not open the port. */
		bind_error?: string | null;
		agent_version?: string;
		/** First agent version whose relay opens a gateway. */
		min_agent_version?: string;
	} | null;
	/**
	 * `websocket`: agents reach their channel through the web address (Cloudflare
	 * Tunnel, a reverse proxy that terminates TLS); `url`, `host` and `port` are
	 * those of the agent WebSocket, and the check went through it.
	 */
	transport?: 'websocket' | null;
	/**
	 * The server accepts agents through the WebSocket of the web address
	 * (`DUMBMONIT_AGENT_WEBSOCKET`): a command can be switched to it.
	 */
	websocket?: boolean;
}

export interface CreateUserPayload {
	username: string;
	display_name?: string;
	role: Role;
	/** A custom role; `role` must then be `viewer`. */
	custom_role_id?: number;
	/** Optional only while single sign-on is enabled. */
	password?: string;
	/** Optional contact address; blank means none. */
	email?: string;
}

export interface UpdateUserPayload {
	display_name?: string;
	/** A built-in role; it removes the custom role. */
	role?: Role;
	/** Gives a custom role (the built-in role becomes `viewer`). */
	custom_role_id?: number;
	disabled?: boolean;
	/** Resets the password and closes the account's sessions. */
	password?: string;
	/** Blank clears it; absent keeps it. */
	email?: string;
}

/** Where the effective OIDC configuration comes from. */
export type OidcSource = 'settings' | 'env' | 'none';

/** `GET /api/auth/oidc/config` — the client secret never comes back. */
export interface OidcConfig {
	source: OidcSource;
	enabled: boolean;
	/** True when environment variables describe a provider (used when nothing is saved). */
	env_available: boolean;
	issuer: string;
	client_id: string;
	has_client_secret: boolean;
	provider_name: string;
	scopes: string;
	auto_create: boolean;
	/** True when `auto_create` was never chosen explicitly: it then defaults to off. */
	auto_create_defaulted: boolean;
	admin_groups: string[];
	/** Groups whose members become operators (unless also in an admin group). */
	operator_groups: string[];
	/** Groups whose members become wall displays (unless also in an admin or operator group). */
	wall_groups: string[];
	groups_claim: string;
	public_url: string;
	/** Computed from `public_url` (or the request origin): register it at the provider. */
	redirect_uri: string;
}

/** `PUT /api/auth/oidc/config` — an empty `client_secret` keeps the stored one. */
export interface OidcConfigPayload {
	issuer: string;
	client_id: string;
	client_secret?: string;
	provider_name: string;
	scopes: string;
	auto_create: boolean;
	admin_groups: string[];
	/** Groups whose members become operators (unless also in an admin group). */
	operator_groups: string[];
	/** Groups whose members become wall displays (unless also in an admin or operator group). */
	wall_groups: string[];
	groups_claim: string;
	public_url: string;
}

/** Endpoints found by `POST /api/auth/oidc/test` (discovery only, no sign-in). */
export interface OidcDiscovery {
	issuer: string;
	authorization_endpoint: string;
	token_endpoint: string;
	jwks_uri: string;
	userinfo_endpoint?: string | null;
	end_session_endpoint?: string | null;
	scopes_supported?: string[] | null;
	id_token_signing_alg_values_supported?: string[] | null;
	code_challenge_methods_supported?: string[] | null;
}

export interface OidcTestReport {
	ok: boolean;
	error?: string;
	discovery?: OidcDiscovery;
}

/** Where a user's groups come from: the `memberOf` attribute, or a group search. */
export type LdapGroupMode = 'memberof' | 'search';

/** `GET /api/auth/ldap/config` — the service account password never comes back. */
export interface LdapConfig {
	source: OidcSource;
	enabled: boolean;
	/** True when `DUMBMONIT_LDAP_*` variables describe a directory (used when nothing is saved). */
	env_available: boolean;
	/** `DUMBMONIT_LDAP_ALLOW_PLAIN` is set: `ldap://` without StartTLS is accepted. */
	allow_plain: boolean;
	url: string;
	starttls: boolean;
	tls_verify: boolean;
	ca_cert: string;
	bind_dn: string;
	has_bind_password: boolean;
	base_dn: string;
	/** Contains `{username}`, escaped by the server (RFC 4515). */
	user_filter: string;
	username_attribute: string;
	display_name_attribute: string;
	email_attribute: string;
	group_mode: LdapGroupMode;
	group_base_dn: string;
	/** Contains `{dn}` and/or `{username}`. */
	group_filter: string;
	admin_groups: string[];
	operator_groups: string[];
	/** Once any group list is filled, only members of a listed group can sign in. */
	viewer_groups: string[];
	/** Members of no other listed group become wall displays. */
	wall_groups: string[];
	timeout_seconds: number;
}

/** `PUT /api/auth/ldap/config` — an empty `bind_password` keeps the stored one (same URL and bind DN). */
export interface LdapConfigPayload {
	url: string;
	starttls: boolean;
	tls_verify: boolean;
	ca_cert: string;
	bind_dn: string;
	bind_password?: string;
	base_dn: string;
	user_filter: string;
	username_attribute: string;
	display_name_attribute: string;
	email_attribute: string;
	group_mode: LdapGroupMode;
	group_base_dn: string;
	group_filter: string;
	admin_groups: string[];
	operator_groups: string[];
	viewer_groups: string[];
	/** Members of no other listed group become wall displays. */
	wall_groups: string[];
	timeout_seconds: number;
}

/** One step of `POST /api/auth/ldap/test`. `detail` is server text (English). */
export interface LdapTestStep {
	step: 'connect' | 'service_bind' | 'user_search' | 'groups';
	ok: boolean;
	detail: string;
	elapsed_ms: number;
}

export interface LdapTestReport {
	ok: boolean;
	steps: LdapTestStep[];
	/** The test user as the directory describes it, and what DumbMonit would do with it. */
	user?: {
		dn: string;
		username: string;
		display_name: string;
		email: string | null;
		groups: string[];
		/** Role at the next sign-in; `null` when access would be refused. */
		role: Role | null;
		allowed: boolean;
	};
}

/**
 * Authentication state as handled by the interface.
 *
 * `available` is not returned by the server: it is `false` when the auth route
 * does not exist yet (404). The instance is then considered unprotected, and
 * the interface stays fully usable.
 */
export interface AuthState {
	available: boolean;
	configured: boolean;
	authenticated: boolean;
	user: User | null;
	oidc: OidcStatus;
	ldap: LdapStatus;
	saml: SamlStatus;
	/** At least one passkey is registered on the instance. */
	passkeys: boolean;
	/** Public demo mode (read-only, fictional data). */
	demo: boolean;
	/** HTTPS on the web port; `null` for a server that does not say. */
	https: HttpsStatus | null;
	/** The instance policy asks this session to set up two-factor first. */
	totpSetupRequired: boolean;
	/** "Forgot password?" emails a reset link (mail server and self-service on). */
	passwordReset?: boolean;
}

/** A passkey (WebAuthn credential) of the current account. Mirrors `auth::passkeys::PasskeyView`. */
export interface Passkey {
	id: number;
	name: string;
	created_at: string;
	last_used_at: string | null;
	/** Copied to a synced keychain (iCloud, Google, a password manager). */
	synced: boolean;
}
