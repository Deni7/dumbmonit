import type { GrantLevel, Role } from './auth.js';

// --- Teams, SAML, SCIM ---------------------------------------------------------

/** Where a team member comes from: added by hand, by an IdP group, or by SCIM provisioning. */
export type MemberSource = 'manual' | 'idp' | 'scim';

export interface TeamMember {
	user_id: number;
	username: string;
	display_name: string;
	role: Role;
	source: MemberSource;
}

/**
 * A right on a folder of devices. `folder` is `"*"` (every device), `""` (devices
 * without a folder) or a folder name (sub-folders `Paris/…` included).
 */
export interface TeamGrant {
	folder: string;
	level: GrantLevel;
}

export interface Team {
	id: number;
	name: string;
	description: string;
	idp_groups: string[];
	members: TeamMember[];
	grants: TeamGrant[];
	created_at: string;
}

/** Absent fields stay unchanged on `PUT`. `members` lists the manual members only. */
export interface TeamInput {
	name?: string;
	description?: string;
	idp_groups?: string[];
	members?: number[];
	grants?: TeamGrant[];
}

export type UnassignedAccess = 'all' | 'none';

export interface TeamsResponse {
	teams: Team[];
	folders: string[];
	settings: { unassigned_access: UnassignedAccess };
}

// --- MSP clients ---------------------------------------------------------------

/** A client in short: the header switcher, `msp` of `GET /api/auth/me`. */
export interface ClientRef {
	id: number;
	name: string;
	has_logo: boolean;
}

/** `msp` of `GET /api/auth/me`. */
export interface MspAccess {
	enabled: boolean;
	/** MSP administrator: sees and manages every client. */
	admin: boolean;
	/** Clients the account can pick: all for the MSP administrator, its own otherwise. */
	clients: ClientRef[];
	/** Client of the current view, if any. */
	current?: number | null;
}

export interface Client {
	id: number;
	name: string;
	description: string;
	/** Recipients of the automatic monthly SLA report. */
	contacts: string[];
	/** Most targets the client may have; `null` for no limit. */
	monitor_quota: number | null;
	idp_groups: string[];
	members: TeamMember[];
	/** Number of targets. */
	targets: number;
	has_logo: boolean;
	sla_report: boolean;
	sla_objective: number | null;
	report_timezone: string;
	/** Last month sent, `YYYY-MM`. */
	report_last_period: string | null;
	report_last_error: string | null;
	created_at: string;
	updated_at: string;
}

/** Absent fields stay unchanged on `PUT`. `members` lists the manual members only. */
export interface ClientInput {
	name?: string;
	description?: string;
	contacts?: string[];
	monitor_quota?: number | null;
	idp_groups?: string[];
	members?: number[];
	sla_report?: boolean;
	sla_objective?: number | null;
	report_timezone?: string;
}

export interface MspSettings {
	enabled: boolean;
}

export interface ClientsResponse {
	settings: MspSettings;
	clients: Client[];
	/** Non-admin accounts in no client: with MSP mode on, they see nothing. */
	unassigned_users: number[];
}

/** `GET /api/clients/overview`: the state of one client. */
export interface ClientHealth {
	id: number;
	name: string;
	has_logo: boolean;
	targets: number;
	monitor_quota: number | null;
	/** Targets with at least one firing alert. */
	alerting_targets: number;
	critical: number;
	warning: number;
	info: number;
	paused: number;
}

/** What can belong to a client. */
export type ClientKind =
	| 'target'
	| 'status_page'
	| 'agent_token'
	| 'channel'
	| 'rule'
	| 'silence'
	| 'report'
	| 'schedule'
	| 'escalation_policy'
	| 'runbook'
	| 'remediation_action';

/**
 * `GET /api/clients/assignments`: per kind, object id → client id. An object
 * missing from its map is shared by the instance. JSON keys are strings.
 */
export type ClientAssignments = Partial<
	Record<
		| 'targets'
		| 'status_pages'
		| 'agent_tokens'
		| 'channels'
		| 'rules'
		| 'silences'
		| 'reports'
		| 'oncall_schedules'
		| 'oncall_policies'
		| 'runbooks'
		| 'remediation_actions',
		Record<string, number>
	>
>;

/** `GET/PUT /api/auth/saml/config`. The two URLs are read-only. */
export interface SamlConfig {
	enabled: boolean;
	idp_entity_id: string;
	idp_sso_url: string;
	/** PEM certificates of the identity provider. */
	idp_certificates: string[];
	sp_entity_id: string;
	allow_idp_initiated: boolean;
	auto_create: boolean;
	button_label: string;
	attr_email: string;
	attr_name: string;
	attr_groups: string;
	admin_groups: string[];
	operator_groups: string[];
	/** Groups whose members become wall displays (unless also in an admin or operator group). */
	wall_groups: string[];
	sp_metadata_url: string;
	acs_url: string;
}

/** `POST /api/auth/saml/metadata/import`: fills the form, nothing is saved. */
export interface SamlMetadataImport {
	idp_entity_id: string;
	idp_sso_url: string;
	idp_certificates: string[];
}

export type ScimDefaultRole = 'viewer' | 'operator' | 'wall';

export interface ScimSettings {
	enabled: boolean;
	base_url: string;
	token_set: boolean;
	token_created_at: string | null;
	default_role: ScimDefaultRole;
}

/** Answer of the token rotation: the bearer token is shown once. */
export interface ScimTokenReply extends ScimSettings {
	token: string;
}

// --- Teams, SAML, SCIM ---------------------------------------------------------

/** Where a team member comes from: added by hand, by an IdP group, or by SCIM provisioning. */
export type MemberSource = 'manual' | 'idp' | 'scim';

export interface TeamMember {
	user_id: number;
	username: string;
	display_name: string;
	role: Role;
	source: MemberSource;
}

/**
 * A right on a folder of devices. `folder` is `"*"` (every device), `""` (devices
 * without a folder) or a folder name (sub-folders `Paris/…` included).
 */
export interface TeamGrant {
	folder: string;
	level: GrantLevel;
}

export interface Team {
	id: number;
	name: string;
	description: string;
	idp_groups: string[];
	members: TeamMember[];
	grants: TeamGrant[];
	created_at: string;
}

/** Absent fields stay unchanged on `PUT`. `members` lists the manual members only. */
export interface TeamInput {
	name?: string;
	description?: string;
	idp_groups?: string[];
	members?: number[];
	grants?: TeamGrant[];
}

export type UnassignedAccess = 'all' | 'none';

export interface TeamsResponse {
	teams: Team[];
	folders: string[];
	settings: { unassigned_access: UnassignedAccess };
}

/** `GET/PUT /api/auth/saml/config`. The two URLs are read-only. */
export interface SamlConfig {
	enabled: boolean;
	idp_entity_id: string;
	idp_sso_url: string;
	/** PEM certificates of the identity provider. */
	idp_certificates: string[];
	sp_entity_id: string;
	allow_idp_initiated: boolean;
	auto_create: boolean;
	button_label: string;
	attr_email: string;
	attr_name: string;
	attr_groups: string;
	admin_groups: string[];
	operator_groups: string[];
	/** Groups whose members become wall displays (unless also in an admin or operator group). */
	wall_groups: string[];
	sp_metadata_url: string;
	acs_url: string;
}

/** `POST /api/auth/saml/metadata/import`: fills the form, nothing is saved. */
export interface SamlMetadataImport {
	idp_entity_id: string;
	idp_sso_url: string;
	idp_certificates: string[];
}

export type ScimDefaultRole = 'viewer' | 'operator' | 'wall';

export interface ScimSettings {
	enabled: boolean;
	base_url: string;
	token_set: boolean;
	token_created_at: string | null;
	default_role: ScimDefaultRole;
}

/** Answer of the token rotation: the bearer token is shown once. */
export interface ScimTokenReply extends ScimSettings {
	token: string;
}
