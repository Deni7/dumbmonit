/**
 * Types of the DumbMonit server API contract.
 *
 * They mirror exactly what the Rust backend returns (`crates/server/src/api`).
 * Any divergence here turns into a silent bug at runtime: these types are not
 * validated at runtime, they describe a promise kept by the server.
 */
import type { LocationId } from './maps.js';

/** Identifier of a target: the SQLite `rowid` on the server side. */
export type TargetId = number;

// --- Health -----------------------------------------------------------------

export interface ComponentHealth {
	ok: boolean;
	error?: string;
	/** VictoriaMetrics only: true when the server runs it itself, false with DUMBMONIT_VM_URL. */
	embedded?: boolean;
}

export interface Health {
	status: 'ok' | 'degraded';
	version: string;
	/** Short commit the server was built from; absent from local builds. */
	build?: string;
	database: ComponentHealth;
	victoria: ComponentHealth;
	/** Public demo mode (`DUMBMONIT_DEMO=1`): read-only, fictional data. */
	demo: boolean;
	/** Data volume, for signed-in callers; absent when the system cannot measure it. */
	data_disk?: { free_bytes: number; total_bytes: number; database_bytes: number };
}

/** One entry of `GET /api/instance/effective-config`: a non-secret setting the server runs with. */
export interface EffectiveSetting {
	/** Stable English identifier, turned into a label by the About section. */
	key: string;
	value: string;
	/** The environment variable that sets it, when there is one. */
	env: string | null;
}

// --- Credentials ------------------------------------------------------------

export type SnmpV3AuthProtocol = 'md5' | 'sha1' | 'sha224' | 'sha256' | 'sha384' | 'sha512';
export type SnmpV3PrivacyProtocol = 'des' | 'aes128' | 'aes192' | 'aes256';

/**
 * A credential as sent on create or edit. Mirrors `CredentialInput` in
 * `crates/proto/src/credential.rs`: on `PUT`, a secret left out (community,
 * passphrase, token `secret`, `password`) keeps the saved one as long as the
 * user name or token ID it belongs to is unchanged.
 */
export type Credential =
	| { type: 'none' }
	| { type: 'snmp_community'; community?: string }
	| {
			type: 'snmp_v3';
			username: string;
			auth?: { protocol: SnmpV3AuthProtocol; passphrase?: string } | null;
			privacy?: { protocol: SnmpV3PrivacyProtocol; passphrase?: string } | null;
			context?: string | null;
	  }
	| { type: 'api_token'; token?: string }
	/**
	 * A Proxmox VE / PBS token in the two pieces the product shows: the server
	 * assembles `token_id=secret` and stores the combined form.
	 */
	| { type: 'api_token'; token_id: string; secret?: string }
	| { type: 'username_password'; username: string; password?: string }
	/** RADIUS: the server's shared secret and a test account; a blank secret keeps the saved one. */
	| { type: 'radius'; username: string; password?: string; shared_secret?: string }
	/** SSH login of a configuration backup; a blank password or enable secret keeps the saved one. */
	| { type: 'ssh_login'; username: string; password?: string; enable_secret?: string }
	/** A family this build does not know: sent as the server described it. */
	| ({ type: string } & Record<string, string>);

/** Credential families this build knows how to label, in display order. */
export const CREDENTIAL_KINDS = [
	{ value: 'snmp_community', label: 'SNMP v1 / v2c (community)' },
	{ value: 'snmp_v3', label: 'SNMP v3' },
	{ value: 'api_token', label: 'API token' },
	{ value: 'username_password', label: 'Username / password' },
	{ value: 'radius', label: 'RADIUS' },
	{ value: 'ssh_login', label: 'SSH login' },
	{ value: 'none', label: 'No authentication' }
] as const;

export type CredentialKind = (typeof CREDENTIAL_KINDS)[number]['value'];

/** Input shapes a credential field can ask for. */
export type CredentialFieldInput = 'text' | 'password' | 'select';

/**
 * One field of a credential form, described by the server.
 *
 * `key` is the property sent in the `credential` object; the server knows how
 * to recompose what it needs from those keys (a Proxmox token from `token_id`
 * + `secret`, an SNMP v3 user from its protocols and passphrases).
 */
export interface CredentialField {
	key: string;
	label: string;
	help: string;
	placeholder: string;
	input: CredentialFieldInput;
	/** Values offered when `input === 'select'`, the first one by default. */
	choices: string[];
	required: boolean;
}

/** A credential family a device type accepts, with the fields to fill in. */
export interface CredentialView {
	/** Value of `credential.type`. */
	kind: string;
	label: string;
	help: string;
	fields: CredentialField[];
}

// --- Targets ----------------------------------------------------------------

/**
 * A target as returned on read.
 *
 * `credential_kind` is a human label ("SNMP community"). The secret itself is
 * never exposed by the API, by design: only whether one is saved
 * (`has_secret`) and the non-secret parts (`credential_public`).
 */
export interface Target {
	id: TargetId;
	name: string;
	address: string;
	kind: string;
	profile_id: string | null;
	parent_id: TargetId | null;
	/** MSP mode: the client of the target (the default client, `1`, otherwise). Older servers omit it. */
	client_id?: number;
	/** Agent that probes this device from its own network; `null` when the server does. */
	via_agent: TargetId | null;
	/** Probed from several locations with a quorum; `null` when only one place probes. */
	multisite: MultisiteConfig | null;
	interval_secs: number;
	enabled: boolean;
	/** Known-vulnerability (CVE) check on for this device; absent from older servers (treat as on). */
	cve_scan?: boolean;
	tags: Record<string, string>;
	credential_kind: string;
	/** A secret is saved: leaving its field blank on edit keeps it. */
	has_secret: boolean;
	/**
	 * Non-secret parts of the credential under the form's field keys
	 * (`username`, `token_id`, `auth_protocol`, `privacy_protocol`, `context`).
	 * Never a secret; empty for a reader.
	 */
	credential_public: Record<string, string>;
	/** Flat folder shown on `/targets`. Empty: no folder. */
	group_name: string;
	/** Free-text note for humans; empty or absent: none. Older servers omit it. */
	note?: string;
	/**
	 * Notification channels of this monitor: an alert its rule sends to every
	 * channel goes to these only. Empty or absent: every channel, as before.
	 */
	channels?: number[];
	/** Creation date (UTC); older servers omit it. */
	created_at?: string | null;
	/** Manual rank, tiebreaking only within the same state and folder. */
	position: number;
	/** Location assigned on the world map; `null`: none (it may still sit at one through its site). */
	location_id: LocationId | null;
	/** Set when Docker labels on an agent's container manage this monitor; `null`: a free monitor. */
	managed?: TargetManaged | null;
	last_probe_at: string | null;
	last_error: string | null;
	/** What `last_error` means: the device is `down`, or our `config` is wrong. */
	error_kind: 'down' | 'config' | null;
}

/** Probing an uptime monitor from several locations. */
export interface MultisiteConfig {
	/** The DumbMonit server itself probes too. */
	server: boolean;
	/** Relay agents (ids of `agent` monitors, same list as `GET /api/relays`) that probe too. */
	agents: number[];
	/** Failing locations needed to declare the monitor down; null = majority of the locations. */
	quorum: number | null;
}

export type ProbeDecision = 'up' | 'degraded' | 'down' | 'unknown';

/** One place that probes a multisite monitor. */
export interface ProbeLocation {
	/** Value of the `location` metric label: 'server' or 'agent-<id>'. */
	location: string;
	agent_id: number | null;
	/** Agent name; empty for the server. */
	name: string;
	site: string | null;
	latitude: number | null;
	longitude: number | null;
	/** excluded: not counted in the quorum; pending: no result yet. */
	state: 'up' | 'down' | 'excluded' | 'pending';
	latency_ms: number | null;
	/** Probe failure reason or exclusion detail (English, from the server). */
	error: string | null;
	excluded_reason: 'silent' | 'not_relay' | 'busy' | 'error' | null;
	checked_at: string | null;
}

/** `GET /api/targets/{id}/probe-locations`. */
export interface ProbeLocationsReport {
	decision: ProbeDecision;
	/** Failing locations needed for "down" (effective, after exclusions). */
	quorum: number;
	/** Locations counted in the last decision. */
	counted: number;
	failing: number;
	/** A confirmation round is running right now. */
	confirming: boolean;
	decided_at: string | null;
	locations: ProbeLocation[];
}

/** Management of a monitor by Docker labels (`dumbmonit.*`). Mirrors `db::targets::Managed`. */
export interface TargetManaged {
	/** Origin: `docker-labels`, or `config-file` for a monitor a configuration file declares. */
	by: string;
	/** Agent whose containers declare the monitor. */
	source: TargetId | null;
	/** Stable key, `project/service/http` or `container/http`. */
	key: string;
	/** Set when no label declares the monitor any more: it is archived (paused, history kept). */
	missing_since: string | null;
}

/** Body sent to `POST /api/targets` and `PUT /api/targets/{id}`. */
export interface TargetPayload {
	name: string;
	address: string;
	kind: string;
	profile_id?: string | null;
	parent_id?: TargetId | null;
	/** MSP mode: the client of the target. Absent: the header's client, the account's only client, or the default one. */
	client_id?: number;
	via_agent?: TargetId | null;
	/** Absent on edit keeps it; `null` turns it off. Exclusive with `via_agent`. */
	multisite?: MultisiteConfig | null;
	interval_secs?: number;
	enabled?: boolean;
	/** CVE check of this device; absent on edit keeps it (on at creation). */
	cve_scan?: boolean;
	tags?: Record<string, string>;
	/** Flat folder shown on `/targets`. Absent or empty: no folder. */
	group_name?: string;
	/** Free-text note (at most 2000 characters). Absent on edit keeps it; empty clears it. */
	note?: string;
	/** Location on the world map; `null`: none. Absent on edit keeps it. */
	location_id?: LocationId | null;
	/** Notification channels (see `Target.channels`). Absent on edit keeps them; `[]`: the rules' channels. */
	channels?: number[];
	credential?: Credential;
}

/**
 * `POST /targets/test`: the form's payload (plus the edited device's `id`, so a
 * blank credential reuses the stored one). Mirrors `TestPayload` in
 * `api/targets.rs`.
 */
export type TargetTestRequest = TargetPayload & { id?: TargetId };

/** Outcome of a probe that saves nothing. Mirrors `TestReport` in `api/targets.rs`. */
export interface TargetTestReport {
	ok: boolean;
	/** The probe's own error message when `ok` is false. */
	error: string | null;
	/** True when the failure means "device down" rather than a wrong setting. */
	unreachable: boolean;
	sysname: string | null;
	sysdescr: string | null;
	profile_id: string | null;
}

/** Where a suggested scan network comes from. Mirrors `SubnetSource` in `api/discovery.rs`. */
export type SubnetSource = 'config' | 'devices' | 'server';

/** One suggested scan network. Mirrors `LocalSubnet` in `api/discovery.rs`. */
export interface LocalSubnet {
	cidr: string;
	source: SubnetSource;
	/** Server interface, `server` source only. */
	interface?: string;
}

/** Result of an immediate probe — the main diagnostic tool. */
export interface ProbeReport {
	sample_count: number;
	series: string[];
}
