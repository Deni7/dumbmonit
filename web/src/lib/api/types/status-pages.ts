import type { TargetId } from './targets.js';

// --- Status pages -----------------------------------------------------------

export type StatusPageTheme = 'auto' | 'light' | 'dark';
/** Accent of a public page: a closed set, each checked for contrast in day and night. */
/** Legacy banner scene ids: still stored and accepted by the server, no longer drawn or edited by the UI. */
export type StatusPageScene = string;
/** Legacy: how a page with several scenes picked the one on screen. */
export type StatusPageSceneRotation = 'visit' | '1m' | '10m' | '1h';

export type StatusPageAccent = 'default' | 'blue' | 'teal' | 'violet' | 'rose' | 'amber';

/** A service shown on a status page, as stored (admin view). */
export interface StatusPageItem {
	id: number;
	page_id: number;
	target_id: TargetId;
	/** Public name of the service. */
	label: string;
	/** Empty string means "no group". */
	group_name: string;
	position: number;
}

/** Search-engine settings of a status page; an empty text means "generated default". */
export interface StatusPageSeo {
	/** `true`: `index, follow`; `false` (default): `noindex, nofollow`. */
	index: boolean;
	title: string;
	description: string;
	keywords: string;
	/** Absolute http(s) URL, or empty. */
	canonical: string;
	/** Share image (Open Graph / Twitter), absolute http(s) URL, or empty. */
	image: string;
	/** Language tag (`en`, `pt-BR`), empty: `en`. */
	lang: string;
	site_name: string;
}

/** Public name of a monitor that reaches a page through a ticked group. */
export interface StatusPageGroupLabel {
	target_id: TargetId;
	label: string;
}

export type StatusPageAccess = 'public' | 'password' | 'users';

export interface StatusPage {
	id: number;
	slug: string;
	title: string;
	description: string;
	published: boolean;
	theme: StatusPageTheme;
	show_uptime_days: number;
	created_at: string;
	updated_at: string;
	accent: StatusPageAccent;
	/** Legacy: kept for compatibility, the UI neither shows nor sends it. */
	scenes: string[];
	/** Legacy: kept for compatibility, the UI neither shows nor sends it. */
	scene_rotation: StatusPageSceneRotation;
	/** Retired: the public page is always plain. Still accepted and returned. */
	simple: boolean;
	/** Public 90-day calendar. */
	show_calendar: boolean;
	/** Public response-time graph per service. */
	show_latency: boolean;
	footer_text: string;
	/** Link back to the organisation's site; empty for none. */
	homepage_url: string;
	/** MIME type of the uploaded logo, `null` without one. */
	logo_type: string | null;
	/** SMTP channel mailing the subscribers instead of the email server; `null`: the email server. */
	subscribe_channel_id: number | null;
	/** Visitors may subscribe by email. */
	subscribe_email: boolean;
	/** Public host name serving this page at `/` (`status.example.com`); `null`: none. */
	domain: string | null;
	seo: StatusPageSeo;
	/** Who may read the page: everyone, whoever knows its password, or signed-in DumbMonit users. */
	access_mode: StatusPageAccess;
	/** A page password is stored (the hash itself never leaves the server). */
	has_password: boolean;
	/** White label: the public page names nothing of DumbMonit; its tab icon is the logo, or none. */
	hide_branding: boolean;
	/** Shown in the public directory (`GET /api/public/status`) when also published and public. */
	listed: boolean;
	/** Checked monitor folders (dynamic): their current members are shown. */
	groups: string[];
	/** Public names of the monitors that come in through a ticked group; none: the monitor name. */
	group_labels: StatusPageGroupLabel[];
	items: StatusPageItem[];
}

export interface StatusPagePayload {
	title: string;
	/** Omit to derive it from the title. */
	slug?: string;
	description?: string;
	published?: boolean;
	theme?: StatusPageTheme;
	show_uptime_days?: number;
	/** Omitted fields below keep their stored value. */
	accent?: StatusPageAccent;
	scenes?: string[];
	scene_rotation?: StatusPageSceneRotation;
	simple?: boolean;
	show_calendar?: boolean;
	show_latency?: boolean;
	footer_text?: string;
	homepage_url?: string;
	/** An SMTP channel sending the subscriber emails; `null`: the email server. Omitted: kept. */
	subscribe_channel_id?: number | null;
	/** Lets visitors subscribe by email. Omitted: kept. */
	subscribe_email?: boolean;
	/** Public host name; `null` or `''` removes it. Omitted: kept. */
	domain?: string | null;
	/** Omitted: kept. */
	seo?: StatusPageSeo;
	/** Checked monitor folders; omitted: kept, `[]`: none. */
	groups?: string[];
	/** Public names of group members; omitted: kept. */
	group_labels?: StatusPageGroupLabel[];
	/** White label (no DumbMonit name or icon on the public page); omitted: kept. */
	hide_branding?: boolean;
	/** Listed in the public directory of status pages; omitted: kept (on for a new page). */
	listed?: boolean;
}

export type StatusSubscriptionKind = 'email' | 'sms' | 'webhook' | 'slack';

/** Subscriber of a status page (admin view; tokens and secrets never leave the server). */
export interface StatusSubscriber {
	id: number;
	page_id: number;
	kind: StatusSubscriptionKind;
	/** Full address for an email; masked number or URL host otherwise. */
	destination: string;
	/** Address, email subscribers only. */
	email: string | null;
	/** Followed device ids; `null`: every service. */
	components: number[] | null;
	/** `null` until the destination was confirmed. */
	confirmed_at: string | null;
	created_at: string;
}

/** Subscription channels open on a page besides email. */
export interface StatusSubscriptionSettings {
	/** Vonage, Telnyx or ASPSMS channel texting the subscribers; `null`: no SMS. */
	sms_channel_id: number | null;
	webhook: boolean;
	slack: boolean;
}

/** Public: what a visitor sends to subscribe. */
export interface StatusSubscribePayload {
	kind: StatusSubscriptionKind;
	email?: string;
	/** E.164, for SMS. */
	phone?: string;
	/** Webhook or Slack incoming webhook URL. */
	url?: string;
	/** Public keys of the followed services; omitted: all. */
	components?: string[];
}

export interface StatusSubscribeReply {
	status: 'pending';
	/** How the destination confirms: a link, a code (SMS) or a signed request (webhook). */
	confirm: 'link' | 'code' | 'signed_request';
	message: string;
	/** Webhook only, shown once: the HMAC-SHA256 signing key. */
	secret?: string;
}

export interface StatusPageItemPayload {
	target_id: TargetId;
	/** Empty falls back to the device name. */
	label?: string;
	group_name?: string;
}

export type IncidentKind = 'incident' | 'maintenance';
export type IncidentSeverity = 'minor' | 'major';
export type IncidentStatus =
	| 'investigating'
	| 'identified'
	| 'monitoring'
	| 'resolved'
	| 'scheduled'
	| 'in_progress'
	| 'completed';

export interface IncidentUpdate {
	id: number;
	incident_id: number;
	status: IncidentStatus;
	body: string;
	created_at: string;
}

export interface Incident {
	id: number;
	/** `null`: shown on every page. */
	page_id: number | null;
	title: string;
	kind: IncidentKind;
	status: IncidentStatus;
	severity: IncidentSeverity;
	starts_at: string;
	ends_at: string | null;
	created_at: string;
	updated_at: string;
	updates: IncidentUpdate[];
	/** Device ids the announcement concerns; empty: the whole page. */
	components: number[];
	/** Draft or published; `null` until one is written. */
	postmortem: Postmortem | null;
}

export interface IncidentPayload {
	title: string;
	kind?: IncidentKind;
	status?: IncidentStatus;
	severity?: IncidentSeverity;
	page_id?: number | null;
	starts_at?: string;
	ends_at?: string;
	/** First message of the timeline (creation only). */
	body?: string;
	/**
	 * Dated messages of the timeline (creation only), to enter a past incident.
	 * An incident cannot start, end or be updated in the future; with `ends_at`
	 * (or a closing update) it is created resolved and nobody is notified.
	 */
	updates?: IncidentNewUpdate[];
	/**
	 * Devices the announcement concerns, among the page's services: subscribers who follow
	 * only other services are not notified. Omitted: unchanged (the whole page at creation).
	 */
	components?: number[];
}

export interface IncidentNewUpdate {
	status: IncidentStatus;
	body: string;
	/** UTC instant; omitted: same as the previous message (the start for the first). */
	at?: string;
}

export interface IncidentUpdatePayload {
	status?: IncidentStatus;
	body: string;
	/** Past date of the message, to complete a history; omitted: now, and subscribers are notified. */
	at?: string;
	/** `false`: publish without notifying subscribers. Omitted: they are notified. */
	notify?: boolean;
}

/** One step of a post-mortem timeline. Mirrors `db::status_pages::TimelineEntry`. */
export interface PostmortemTimelineEntry {
	/** UTC, `YYYY-MM-DDTHH:MM:SSZ`. */
	at: string;
	text: string;
}

/** Mirrors `db::status_pages::ActionItem`. */
export interface PostmortemAction {
	text: string;
	done: boolean;
}

/** Post-mortem of an incident. Mirrors `db::status_pages::Postmortem`. */
export interface Postmortem {
	incident_id: number;
	summary: string;
	impact: string;
	root_cause: string;
	/** Oldest first. */
	timeline: PostmortemTimelineEntry[];
	actions: PostmortemAction[];
	published: boolean;
	/** First publication; `null` while a draft. */
	published_at: string | null;
	created_at: string;
	updated_at: string;
}

/** Body of `PUT /api/incidents/{id}/postmortem`: a full replacement. Publishing needs a resolved incident. */
export interface PostmortemPayload {
	summary: string;
	impact: string;
	root_cause: string;
	/** Any order; `at` is RFC 3339 (UTC). */
	timeline: PostmortemTimelineEntry[];
	actions: PostmortemAction[];
	published: boolean;
}

/** What a public page shows of a published post-mortem. Mirrors `PublicPostmortem`. */
export interface PublicPostmortem {
	summary: string;
	impact: string;
	root_cause: string;
	timeline: PostmortemTimelineEntry[];
	actions: PostmortemAction[];
	published_at: string | null;
}

// --- Service level objectives (`/api/slos`, mirrors `crates/server/src/slo`) ---

/** Rolling windows an SLO can use, in days. Mirrors `slo::WINDOW_DAYS`. */
export const SLO_WINDOWS = [7, 28, 30, 90] as const;
export type SloWindow = (typeof SLO_WINDOWS)[number];

/** Mirrors `slo::Budget`. Remaining values turn negative once the target is missed. */
export interface SloBudget {
	total_minutes: number;
	consumed_minutes: number;
	remaining_minutes: number;
	remaining_percent: number;
}

/** Burn rates over 5 min, 30 min, 1 h and 6 h; `null` without measurements. Mirrors `slo::BurnRates`. */
export interface SloBurnRates {
	m5: number | null;
	m30: number | null;
	h1: number | null;
	h6: number | null;
}

/** `fast`: 14.4× over 1 h and 5 min; `slow`: 6× over 6 h and 30 min. Mirrors `slo::Burn`. */
export type SloVerdict = 'none' | 'slow' | 'fast';

/** Mirrors `slo::SloStatus`. */
export interface SloStatus {
	/** False when VictoriaMetrics did not answer. */
	measured: boolean;
	monitors: number;
	/** Availability over the window, in percent. */
	attained: number | null;
	budget: SloBudget | null;
	burn: SloBurnRates;
	verdict: SloVerdict;
}

/** Mirrors `api::slos::SloView`. */
export interface Slo {
	id: number;
	name: string;
	description: string;
	/** Availability target in percent, strictly between 0 and 100. */
	objective: number;
	window_days: number;
	/** Same shape as an SLA report scope; empty: every monitor. */
	scope: { targets: number[]; groups: string[] };
	enabled: boolean;
	created_at: string;
	updated_at: string;
	status: SloStatus;
}

/** Mirrors `api::slos::SloDetail`. */
export interface SloDetail extends Slo {
	members: number[];
	/** `[unix seconds, % of the budget left]`, starting at 100. */
	burndown: [number, number][];
}

/** Body of `POST` and `PUT /api/slos`. */
export interface SloPayload {
	name: string;
	description: string;
	objective: number;
	window_days: number;
	scope: { targets: number[]; groups: string[] };
	enabled: boolean;
}

// Public document of `GET /api/public/status/{slug}`: no ids, no addresses.

export type PublicOverall = 'operational' | 'degraded' | 'major' | 'maintenance';
export type PublicItemState = 'up' | 'down' | 'degraded' | 'maintenance' | 'unknown';

export interface PublicDayBucket {
	/** `YYYY-MM-DD`, UTC. */
	date: string;
	/** `null` without any measurement that day. */
	uptime_pct: number | null;
	/** Minutes of measured downtime that day; `null` without any measurement. */
	down_minutes: number | null;
	incidents: number;
}

/** `[unix seconds, milliseconds]` points, averaged per slice server-side. */
export interface PublicLatencySeries {
	h24: [number, number][];
	d7: [number, number][];
}

export type PublicCalendarState = 'operational' | 'degraded' | 'outage' | 'nodata';

export interface PublicCalendarDay {
	/** `YYYY-MM-DD`, UTC. */
	date: string;
	state: PublicCalendarState;
	/** Uptime of the least available service that day; `null` without data. */
	uptime_pct: number | null;
	incidents: number;
}

export interface PublicStatusItem {
	/** Stable public key of the service (from its label), used in badge URLs. */
	key: string;
	/** Editor preview only (`GET /api/status-pages/{id}/preview`): the monitor shown. */
	target_id?: TargetId;
	label: string;
	state: PublicItemState;
	uptime_24h: number | null;
	uptime_7d: number | null;
	uptime_30d: number | null;
	/** Only when the page shows 90 days of history. */
	uptime_90d: number | null;
	/** Over the page's `history_days`: the same days as `history`. */
	uptime_history: number | null;
	latency_ms: number | null;
	/** Only when the page shows response times and the service has latency data. */
	latency_series?: PublicLatencySeries;
	history: PublicDayBucket[];
	/** A third-party provider (a `provider` monitor): shown as an external component. */
	external?: boolean;
}

/** The admin-written texts of a status page in one language; empty keeps the written text. */
export interface StatusPageTranslation {
	lang: string;
	title: string;
	description: string;
	footer_text: string;
	/** Written service or group name to its translation. */
	labels: Record<string, string>;
}

/** `GET`/`PUT /api/status-pages/{id}/languages`. */
export interface StatusPageLanguages {
	/** Default interface language of the page; `""` is English. */
	default_lang: string;
	translations: StatusPageTranslation[];
}

/** One day of a status page, hour by hour (`GET /api/public/status/{slug}/day/{date}`). */
export interface PublicDayDetail {
	date: string;
	services: {
		key: string;
		label: string;
		/** Availability of each UTC hour; `null` without a measurement. */
		hours: (number | null)[];
		outages: { from: string; to: string; minutes: number }[];
	}[];
	incidents: { title: string; kind: IncidentKind; status: IncidentStatus; starts_at: string; ends_at: string | null }[];
}

export interface PublicStatusGroup {
	name: string;
	items: PublicStatusItem[];
}

export interface PublicIncidentUpdate {
	status: IncidentStatus;
	body: string;
	created_at: string;
}

export interface PublicIncident {
	/** Anchor and permalink: `#incident-<anchor>` (start and title in a slug, never the internal id). */
	anchor: string;
	/** Public keys of the services touched; absent: the whole page. */
	components?: string[];
	title: string;
	kind: IncidentKind;
	status: IncidentStatus;
	severity: IncidentSeverity;
	starts_at: string;
	ends_at: string | null;
	updates: PublicIncidentUpdate[];
	/** Published post-mortem of a resolved incident; absent otherwise. */
	postmortem?: PublicPostmortem;
}

export interface PublicStatus {
	page: {
		slug: string;
		title: string;
		description: string;
		theme: StatusPageTheme;
		/** The page's setting: the longest window it shows (30, 60 or 90). */
		show_uptime_days: number;
		/** The window actually shown: the age of the data, from 7 days up to `show_uptime_days`. */
		history_days: number;
		/** What the history bars really cover, in seconds: the chosen period, or less for younger data. */
		window_secs: number;
		/** Duration of one bar in seconds; 86400 = one bar per day. */
		step_secs: number;
		updated_at: string;
		accent: StatusPageAccent;
		/** Legacy: no longer drawn by the UI. */
		scenes: string[];
		scene_rotation: StatusPageSceneRotation;
		/** Retired: no effect any more. Still accepted and returned. */
		simple: boolean;
		/** Present (true) only when the page shows the calendar / response times. */
		show_calendar?: boolean;
		show_latency?: boolean;
		footer_text: string;
		homepage_url: string;
		/** Versioned relative URL of the logo (in MSP mode, the client's logo when the page has none), `null` without one. */
		logo_url: string | null;
		/** MSP mode: name of the page's client. */
		brand_name?: string;
		/** The page offers at least one subscription. */
		subscribe: boolean;
		/** Subscriptions offered. */
		subscribe_kinds: StatusSubscriptionKind[];
		/** White label: no DumbMonit icon or name around the page. Older servers omit it. */
		hide_branding?: boolean;
		/** Default interface language of the page, used when the visitor's browser speaks none of ours. */
		default_lang?: string;
		/** Language the page's own texts were served in. */
		lang?: string;
		/** Languages the admin translated the page's texts into. */
		languages?: string[];
		/** Effective search-engine values (settings, or defaults built from the page name). */
		seo?: {
			indexable: boolean;
			title: string;
			description: string;
			keywords: string;
			canonical?: string;
			image?: string;
			lang: string;
			site_name: string;
		};
	};
	overall: PublicOverall;
	generated_at: string;
	groups: PublicStatusGroup[];
	/** 90 days, oldest first; present only when the page shows the calendar. */
	calendar?: PublicCalendarDay[];
	/** Open incidents and those of the last 30 days. */
	incidents: PublicIncident[];
	/** Scheduled or in-progress maintenance windows. */
	maintenance: PublicIncident[];
}

/** One slice of past incidents older than the page's 30 days. */
export interface PublicOlderIncidents {
	incidents: PublicIncident[];
	/** Cursor of the next slice; absent at the end. */
	next?: string;
}
