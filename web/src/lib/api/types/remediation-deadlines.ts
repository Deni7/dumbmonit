import type { TargetId } from './targets.js';

// --- Runbooks and remediation (`crates/server/src/api/remediation.rs`) -------

export type RunbookScope = 'rule' | 'group' | 'kind';

export interface RunbookLink {
	scope: RunbookScope;
	/** Rule uid, folder name or device kind. */
	value: string;
}

export interface Runbook {
	id: number;
	title: string;
	/** Markdown, rendered with HTML escaped (`#lib/markdown.ts`). */
	body: string;
	links: RunbookLink[];
	created_at: string;
	updated_at: string;
	updated_by: string | null;
}

export interface RunbookInput {
	title: string;
	body: string;
	links: RunbookLink[];
}

export type RemediationKindId = 'service.restart' | 'container.restart' | 'script.run' | 'webhook';

export interface RemediationKindField {
	key: string;
	label: string;
	input: 'text' | 'url' | 'secret' | 'params';
	required: boolean;
	help: string;
}

/** One kind of the closed catalogue, described by the server (English). */
export interface RemediationKind {
	kind: string;
	label: string;
	description: string;
	needs_agent: boolean;
	fields: RemediationKindField[];
}

export interface RemediationAction {
	id: number;
	name: string;
	kind: string;
	/** Non-secret parameters (`service`, `container`, `script` + `params`, `url`). */
	params: Record<string, unknown>;
	/** A secret (webhook token) is stored; it is never sent back. */
	has_secret: boolean;
	target_id: TargetId | null;
	created_at: string;
	updated_at: string;
}

export interface RemediationActionInput {
	name: string;
	kind: string;
	params: Record<string, unknown>;
	target_id: TargetId | null;
	/** Omitted: kept. `{}`: cleared. */
	secrets?: Record<string, string>;
}

export interface RemediationPolicy {
	id: number;
	action_id: number;
	action_name: string;
	action_kind: string;
	rule_uid: string;
	target_id: TargetId | null;
	after_minutes: number;
	max_attempts: number;
	window_minutes: number;
	/** Armed (confirmed in two steps). */
	enabled: boolean;
	armed_by: string | null;
	armed_at: string | null;
	/** Set after two failed runs in a row. */
	halted_at: string | null;
	halt_reason: string | null;
	created_at: string;
	updated_at: string;
}

export interface RemediationPolicyInput {
	action_id: number;
	rule_uid: string;
	target_id: TargetId | null;
	after_minutes: number;
	max_attempts: number;
	window_minutes: number;
}

export interface ArmChallenge {
	token: string;
	expires_in_secs: number;
	summary: string;
	policy: RemediationPolicy;
}

export type RemediationRunStatus = 'queued' | 'running' | 'done' | 'failed' | 'cancelled' | 'expired';

export interface RemediationRun {
	id: number;
	action_id: number | null;
	action_name: string;
	kind: string;
	params: Record<string, unknown>;
	target_id: TargetId | null;
	target_name: string | null;
	fingerprint: string | null;
	rule_uid: string | null;
	policy_id: number | null;
	trigger: 'manual' | 'auto';
	requested_by: string;
	status: RemediationRunStatus;
	/** Truncated to 4 KiB, end kept. */
	output: string | null;
	created_at: string;
	finished_at: string | null;
}

export interface AlertRemediation {
	runbooks: Runbook[];
	actions: RemediationAction[];
	policies: RemediationPolicy[];
	runs: RemediationRun[];
}

export interface DeclaredScript {
	name: string;
	description: string;
	params: Record<string, string[]>;
	timeout_secs: number;
}

export interface TargetRemediation {
	runbooks: Runbook[];
	agent: {
		host_id: TargetId;
		commands_enabled: boolean;
		remediation: { enabled: boolean; services: string[]; scripts: DeclaredScript[] } | null;
	} | null;
	runs: RemediationRun[];
}

// --- Deadlines (crates/server/src/api/deadlines.rs) ----------------------------
// Expiry dates collected across the instance: domains, certificates,
// application secrets, products, device keys, licences.

export type DeadlineKind =
	| 'domain'
	| 'certificate'
	| 'credential'
	| 'product'
	| 'key'
	| 'license'
	/** Only with `upcoming`: full on that date. */
	| 'disk'
	/** Only with `upcoming`: late on that date. */
	| 'backup';

export interface Deadline {
	target: TargetId;
	target_name: string;
	target_kind: string;
	/** Unknown kinds are rendered as is. */
	kind: DeadlineKind | (string & {});
	/** The object that expires: host, application, device… */
	what: string;
	/** UTC, no suffix (`parseServerDate` adds the `Z`). */
	expires_at: string;
	/** Negative once expired. */
	days_left: number;
	/** `disk`: used, in percent; `backup`: age of the last success, in days. */
	value?: number;
}
