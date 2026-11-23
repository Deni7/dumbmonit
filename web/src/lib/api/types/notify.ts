import type { AlertSeverity, SilenceSchedule } from './alerts.js';
import type { TargetId } from './targets.js';

// --- Notification channels --------------------------------------------------

/** A form field described by the server (`GET /api/notify/kinds`). */
export interface ChannelField {
	/** Key in `settings` or `secrets`. */
	key: string;
	label: string;
	required: boolean;
	input: 'text' | 'url' | 'number' | 'boolean' | 'select' | 'textarea' | 'password';
	help: string;
	placeholder: string;
	/** Values offered when `input === 'select'`. */
	options: string[];
	/**
	 * How the value is stored: `list` is one entry per line (recipients),
	 * `object` is a JSON object (headers, extra data). Scalar otherwise.
	 */
	shape: 'scalar' | 'list' | 'object';
	/** Value applied by the server when the field is left empty. */
	default: string;
	/**
	 * The field says where the secrets go (server address, SMTP host, port...).
	 * Changing it on edit means typing the secrets again: the server refuses to
	 * send stored secrets to a new destination.
	 */
	destination?: boolean;
	/**
	 * Names a list the UI fetches to offer as choices instead of a free value
	 * (`oncall_policies`: the escalation policies). Absent or empty otherwise.
	 */
	lookup?: string;
}

/** A channel type (Discord, Telegram...) and the fields it expects. */
export interface ChannelKindInfo {
	kind: string;
	label: string;
	summary: string;
	/** Documentation link, for example `https://dumbmonit.selfhostr.io/docs/notifications/#discord`. */
	doc_url: string;
	settings: ChannelField[];
	secrets: ChannelField[];
	/** How the channel shows Acknowledge / Snooze links: native buttons, links in the
	 *  message, or not at all (services with their own acknowledgement). */
	actions?: 'buttons' | 'links' | 'none' | string;
	/** Format of the channel's message templates; empty when the kind takes none. */
	template_format?: MessageTemplateFormat | '';
}

/** How a channel's message templates are written, which decides how values are escaped. */
export type MessageTemplateFormat = 'plain' | 'slack_mrkdwn' | 'html' | 'telegram_markdown_v2';

/** `POST /api/notify/template-preview`: empty templates render the defaults. */
export interface TemplatePreviewRequest {
	kind: string;
	title_template?: string | null;
	body_template?: string | null;
}

/** A channel's message templates rendered on a sample disk-full alert. */
export interface TemplatePreview {
	format: MessageTemplateFormat;
	title: string;
	/** In the service's format, values escaped. */
	body: string;
	default_title: string;
	default_body: string;
}

/** A channel as returned on read: never its secrets. */
export interface Channel {
	id: number;
	name: string;
	kind: string;
	enabled: boolean;
	settings: Record<string, unknown>;
	has_secret: boolean;
	last_error: string | null;
	last_sent_at: string | null;
	policy: ChannelPolicy;
	/** With interactive buttons (Slack, Telegram) and a public URL: where the platform
	 *  sends button presses. Absent otherwise. */
	callback_url?: string;
}

/**
 * Body of `POST /api/notify/channels` and `PUT /api/notify/channels/{id}`.
 *
 * Omitting `secrets` on an update keeps the stored secrets; `{}` explicitly
 * clears them. Otherwise keys are merged: a key sent replaces its value, a key
 * left out is kept, and `null` or `""` clears it. A change of kind or
 * destination replaces the whole set instead.
 */
export interface ChannelPayload {
	name: string;
	kind: string;
	enabled?: boolean;
	settings?: Record<string, unknown>;
	secrets?: Record<string, unknown>;
	/** Omitted on an update keeps the stored policy; fields absent keep their value. */
	policy?: ChannelPolicyPayload;
}

export interface ChannelTestReport {
	ok: boolean;
	message: string;
	/** Settings that would hold back a real alert on this channel (channel tests only). */
	filters?: string[];
}

// --- Notification policy ----------------------------------------------------

/** Weekly quiet-hours window, same shape as a weekly maintenance schedule. */
export type QuietHours = Extract<SilenceSchedule, { kind: 'weekly' }>;

/**
 * One condition of a channel's routing filter.
 *
 * Conditions on the same field (the same tag key, or `kind`, or `rule`) read as
 * OR; different fields read as AND. Exclusions always win.
 */
export type MatchCondition =
	| { field: 'tag'; key: string; value: string }
	| { field: 'kind'; value: string }
	| { field: 'rule'; value: string };

/** What a channel accepts. Both lists empty: it receives everything. */
export interface ChannelMatcher {
	include: MatchCondition[];
	exclude: MatchCondition[];
}

/** One device as `POST /api/notify/match-preview` returns it. */
export interface MatchPreviewDevice {
	id: TargetId;
	name: string;
	kind: string;
	tags: Record<string, string>;
	matched: boolean;
}

/** Response of `POST /api/notify/match-preview`. */
export interface MatchPreview {
	devices: MatchPreviewDevice[];
	matched: number;
	total: number;
	/** Rule uids the filter requires; a device list carries no rule, so the UI
	 *  says them out loud instead of pretending they are ignored. */
	rules: string[];
	excluded_rules: string[];
}

/** Per-channel policy, mirroring the server's `ChannelPolicy`. */
export interface ChannelPolicy {
	/** Alerts below this severity never reach the channel. */
	min_severity: AlertSeverity;
	/** False: the channel only hears about problems, never about recoveries. */
	notify_resolved: boolean;
	/** Minimum seconds between two messages about the same alert; 0 = none. */
	min_interval_secs: number;
	/** During quiet hours only `critical` goes through; the rest waits for a digest. */
	quiet_hours: QuietHours | null;
	/** What the channel accepts. Empty lists: everything, as before. */
	matcher: ChannelMatcher;
}

/** Body of the `policy` field on a channel: every field optional. */
export interface ChannelPolicyPayload {
	min_severity?: AlertSeverity | null;
	notify_resolved?: boolean | null;
	min_interval_secs?: number | null;
	/** `null` clears the quiet hours; omitted keeps them. */
	quiet_hours?: QuietHours | null;
	/** `null` clears the routing filter; omitted keeps it. */
	matcher?: ChannelMatcher | null;
}

/** Global policy, `GET/PUT /api/notify/policy`. */
export interface NotificationPolicy {
	/** Alerts firing within this window leave as one message per channel; 0 = immediately. */
	batch_window_secs: number;
	/** Messages per channel per hour; 0 = unlimited. */
	max_per_hour: number;
	/** State changes in `flap_window_secs` after which an alert is held; 0 = off. */
	flap_events: number;
	flap_window_secs: number;
	flap_hold_secs: number;
	/** Public URL used for device links in messages; empty falls back to DUMBMONIT_PUBLIC_URL. */
	public_url: string;
	/** Seconds without an acknowledgement after which the alert is also sent to
	 *  `escalate_channel`; 0 = no escalation. */
	escalate_after_secs: number;
	/** Channel told second. `null` = no escalation, whatever the delay. */
	escalate_channel: number | null;
	/** Acknowledge / Snooze links in notifications; only effective with a public URL.
	 *  Switching it off also revokes the links already sent. */
	action_links: boolean;
	/** Recovery of an announced alert while a silence or maintenance window covers it:
	 *  `suppress` (default) sends nothing, `send` notifies anyway. */
	resolutions_during_silence: 'suppress' | 'send';
}

/** What a notification action link does (`/alert-action#<token>`). */
export type AlertLinkAction = 'ack' | 'snooze' | 'dismiss';

/** An alert named by an action link, reduced to what is needed to decide. */
export interface AlertLinkTarget {
	rule_name: string;
	target_name: string | null;
	severity: AlertSeverity;
	acked: boolean;
	acked_until: string | null;
	dismissed: boolean;
}

/** `POST /api/public/alert-action/inspect`: what the link would do. */
export interface AlertLinkView {
	action: AlertLinkAction;
	expires_at: string;
	used: boolean;
	/** The link's alerts that are still active. */
	alerts: AlertLinkTarget[];
}

/** `POST /api/public/alert-action`: what the link did. */
export interface AlertLinkResult {
	action: AlertLinkAction;
	/** The instant of the gesture (a dismissal) or the end of the ack / snooze. */
	until: string;
	acked: number;
	/** Alerts dismissed by the link (`acked` is 0 then). */
	dismissed: number;
	alerts: AlertLinkTarget[];
}

export type NotificationPolicyPayload = Partial<NotificationPolicy>;

/** A rule's per-device override; a `null` field keeps the rule's value. */
export interface RuleOverride {
	rule_uid: string;
	target_id: TargetId;
	threshold: number | null;
	clear_threshold: number | null;
	enabled: boolean | null;
}

/** A rule that watches one device: it reads a metric the device has written. Mirrors `api/target_rules.rs`. */
export interface TargetRule {
	id: number;
	uid: string;
	name: string;
	description: string;
	severity: AlertSeverity;
	/** Enabled for every device. */
	enabled: boolean;
	/** Turned off for this device (an override with `enabled: false`). */
	ignored: boolean;
	/** An administrator set a threshold for this device. */
	custom_threshold: boolean;
	/** Firing on this device right now. */
	firing: boolean;
}

export interface TargetRules {
	/** `no_data`: nothing written in the last 24 h; `unavailable`: the time-series store did not answer. */
	status: 'ok' | 'no_data' | 'unavailable';
	rules: TargetRule[];
}

export interface RuleOverridePayload {
	threshold?: number | null;
	clear_threshold?: number | null;
	enabled?: boolean | null;
}
