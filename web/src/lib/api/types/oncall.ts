import type { Role } from './auth.js';
import type { QuietHours } from './notify.js';

// --- On-call (schedules, escalation, personal contacts) ------------------------
// Mirrors `crates/server/src/api/oncall*.rs`. Times on the wire are RFC 3339
// UTC with a `Z`; a rotation start is a naive local `YYYY-MM-DDTHH:MM` in the
// schedule's own timezone.

/** A person who can be put on a rotation. */
export interface Person {
	id: number;
	display_name: string;
	disabled: boolean;
	/** On call right now on at least one schedule. */
	on_call: boolean;
	/** The fields below are only sent to administrators and to the person themselves. */
	username?: string;
	role?: Role;
	/** Enabled contact methods. */
	contacts?: number;
}

/** Hours a layer is active, 0 = Monday … 6 = Sunday. `end <= start` crosses midnight; both 0 = all day. */
export interface Window {
	days: number[];
	start_minute: number;
	end_minute: number;
}

export type RotationKind = 'daily' | 'weekly' | 'custom';
export type RotationUnit = 'hours' | 'days' | 'weeks';

export interface Layer {
	name: string;
	rotation: {
		kind: RotationKind;
		/** Local `YYYY-MM-DDTHH:MM`: first handoff (time = handoff time, weekday = weekly handoff day). */
		start: string;
		/** Custom only (>= 1). */
		length: number;
		unit: RotationUnit;
	};
	/** User ids in rotation order (repeats allowed). */
	participants: number[];
	/** Empty: active all the time. */
	restrictions: Window[];
}

export interface OnCallNow {
	user_id: number;
	since: string | null;
	until: string | null;
	layer: number | null;
	override_id: number | null;
}

export interface Schedule {
	id: number;
	name: string;
	description: string;
	/** IANA name. */
	timezone: string;
	handoff_notifications: boolean;
	layers: Layer[];
	now: OnCallNow | null;
	created_at: string;
	updated_at: string;
}

export interface SchedulePayload {
	name: string;
	description?: string;
	timezone: string;
	handoff_notifications?: boolean;
	layers: Layer[];
}

export interface Shift {
	start: string;
	end: string;
	user_id: number;
	layer: number | null;
	override_id: number | null;
}

/** "X replaces Y from ... to ...". */
export interface Override {
	id: number;
	schedule_id: number;
	/** Who takes the shift. */
	user_id: number;
	/** Only when that person would be on call; `null`: whoever is. */
	replaces: number | null;
	starts_at: string;
	ends_at: string;
	note: string;
	created_by: string;
	created_at: string;
}

export interface OverridePayload {
	user_id: number;
	replaces?: number | null;
	starts_at: string;
	ends_at: string;
	note?: string;
}

export interface ShiftsResponse {
	final: Shift[];
	/** One list per layer, same order as the schedule's layers. */
	layers: Shift[][];
	overrides: Override[];
}

export interface OnCallEntry {
	schedule_id: number;
	schedule_name: string;
	user_id: number | null;
	since: string | null;
	until: string | null;
	override_id: number | null;
}

export type StepTarget = { type: 'schedule'; id: number } | { type: 'user'; id: number } | { type: 'channel'; id: number };

export interface Step {
	/** 1..1440: wait after this step before the next one. */
	delay_minutes: number;
	targets: StepTarget[];
}

export interface EscalationPolicy {
	id: number;
	name: string;
	description: string;
	/** 0..9 extra rounds after the last step. */
	repeat: number;
	steps: Step[];
	/** Ids of the "oncall" channels that use this policy. */
	channels: number[];
	created_at: string;
	updated_at: string;
}

export interface EscalationPolicyPayload {
	name: string;
	description?: string;
	repeat?: number;
	steps: Step[];
}

export type EscalationStatus = 'active' | 'acked' | 'resolved' | 'exhausted' | 'cancelled';
export type EscalationEventKind = 'started' | 'notified' | 'failed' | 'skipped' | 'acked' | 'resolved' | 'exhausted' | 'cancelled';

export interface EscalationEvent {
	at: string;
	step: number | null;
	round: number | null;
	event: EscalationEventKind;
	user_id: number | null;
	user_name: string | null;
	/** Contact label or channel name. */
	via: string;
	/** Channel kind. */
	kind: string;
	detail: string;
}

export interface Escalation {
	id: number;
	policy_id: number;
	policy_name: string;
	channel_id: number;
	fingerprint: string;
	target_id: number | null;
	target_name: string;
	rule_name: string;
	severity: 'info' | 'warning' | 'critical';
	status: EscalationStatus;
	/** Next step index. */
	step: number;
	round: number;
	next_at: string | null;
	started_at: string;
	ended_at: string | null;
	acked_by: string | null;
	events: EscalationEvent[];
}

/** A personal way to be reached: through a team channel, or a service of one's own. */
export interface Contact {
	id: number;
	label: string;
	kind: string;
	via_channel_id: number | null;
	via_channel_name: string | null;
	address: string | null;
	settings: Record<string, unknown>;
	has_secret: boolean;
	enabled: boolean;
	min_severity: 'info' | 'warning' | 'critical';
	last_error: string | null;
	last_used_at: string | null;
}

/** Omitting `secrets` on an update keeps them; `{}` clears them. */
export interface ContactPayload {
	label: string;
	kind?: string;
	via_channel_id?: number | null;
	address?: string | null;
	settings?: Record<string, unknown>;
	secrets?: Record<string, unknown>;
	enabled?: boolean;
	min_severity?: 'info' | 'warning' | 'critical';
}

export interface ContactOptions {
	/** Channel kinds allowed for a personal service. */
	kinds: string[];
	team_channels: { id: number; name: string; kind: string; recipient: 'email' | 'phone' | 'chat_id' }[];
}

export interface OnCallProfile {
	timezone: string;
	quiet_hours: QuietHours | null;
	/** An iCal link exists. */
	calendar: boolean;
	on_call: OnCallEntry[];
	next_shift: { schedule_id: number; schedule_name: string; start: string; end: string } | null;
}

export interface OnCallProfilePayload {
	timezone?: string;
	/** `null` clears. */
	quiet_hours?: QuietHours | null;
}
