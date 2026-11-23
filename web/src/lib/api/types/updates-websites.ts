// --- Updates ----------------------------------------------------------------

/** `GET /api/update`: mirrors `UpdateInfo` in `crates/server/src/update.rs`. */
export interface UpdateInfo {
	/** Running version. */
	current: string;
	/** Latest release known; null before the first successful check. */
	latest: string | null;
	release_url: string | null;
	/** Release notes, shortened by the server. */
	notes: string | null;
	update_available: boolean;
	/** Last successful check, server timestamp (UTC without suffix). */
	checked_at: string | null;
	/** The check is active. */
	enabled: boolean;
	/** Turned off by `DUMBMONIT_UPDATE_CHECK=off`: the switch cannot override it. */
	locked_by_env: boolean;
	/** Why the last check failed. */
	error: string | null;
}

// --- Configuration changes after an update ----------------------------------

/** Where a change stands for this instance (`Status` in `crates/server/src/upgrade_notes.rs`). */
export type UpgradeNoteStatus = 'pending' | 'closed' | 'clear';

/** Verdict of an automatic check: the change is in place, missing, or cannot be told from here. */
export type UpgradeDetectionState = 'ok' | 'missing' | 'unknown';

export interface UpgradeDetection {
	state: UpgradeDetectionState;
	/** Why, in a sentence (server text). */
	detail: string;
}

/** One change a version asks for. Mirrors `NoteView` in `crates/server/src/upgrade_notes.rs`. */
export interface UpgradeNote {
	id: string;
	/** Version that introduced it. */
	version: string;
	title: string;
	/** What changed and why (server text). */
	summary: string;
	/** `docker-compose.yml` block to copy. */
	compose: string;
	/** Command to run afterwards. */
	apply: string | null;
	docs_url: string | null;
	status: UpgradeNoteStatus;
	/** Automatic check, for a change that has one and is not closed. */
	detection: UpgradeDetection | null;
	/** Server timestamp (UTC without suffix). */
	closed_at: string | null;
}

/** `GET /api/upgrade-notes`: mirrors `Overview` in `crates/server/src/upgrade_notes.rs`. */
export interface UpgradeNotes {
	/** Running version. */
	current: string;
	/** Version the instance came from. */
	upgraded_from: string | null;
	/** The window should open by itself (changes pending, not yet shown for this version). */
	announce: boolean;
	/** Changes to apply. */
	pending: number;
	/** Every known change: pending first, then closed, then the rest. */
	notes: UpgradeNote[];
}

// --- Bulk import of websites (crates/server/src/api/targets_import.rs) -------

export type TargetImportStatus = 'ok' | 'created' | 'skipped' | 'error';

export interface TargetImportRow {
	/** Line number in the file (the header is line 1). */
	row: number;
	url: string;
	name: string;
	folder: string;
	platform: string | null;
	/** Category from the CSV column, else guessed from the page. */
	category: string | null;
	status: TargetImportStatus;
	message: string | null;
	id: number | null;
}

/** Answer to `POST /api/targets/import`. */
export interface TargetImportReport {
	dry_run: boolean;
	total: number;
	created: number;
	skipped: number;
	errors: number;
	rows: TargetImportRow[];
}

/** One monitor of an Uptime Kuma backup. Mirrors `crates/server/src/api/kuma_import.rs`. */
export interface KumaImportRow {
	/** Position of the monitor in the file, from 1. */
	row: number;
	name: string;
	/** Uptime Kuma monitor type. */
	source_type: string;
	/** Device type created; null when the monitor is skipped. */
	kind: string | null;
	address: string;
	folder: string;
	interval_secs: number | null;
	/** Uptime Kuma tags as `key=value`. */
	tags: string[];
	status: TargetImportStatus;
	/** Why it was skipped, or what is not imported (English, as served). */
	message: string | null;
	id: number | null;
}

/** Answer to `POST /api/targets/import/kuma`. */
export interface KumaImportReport {
	dry_run: boolean;
	total: number;
	created: number;
	skipped: number;
	errors: number;
	notifications_ignored: number;
	rows: KumaImportRow[];
}

// --- Website health & SEO (kind `website`) ----------------------------------
// Mirrors `crates/server/src/api/website.rs`.

export interface WebsiteHop {
	url: string;
	status: number;
}

export interface WebsiteMetadata {
	title: string | null;
	description: string | null;
	canonical: string | null;
	robots: string | null;
	og_title: string | null;
	og_description: string | null;
	lang: string | null;
	h1_count: number;
	hreflang_count: number;
	/** Languages of the hreflang alternates, sorted. */
	hreflang: string[];
	/** google-site-verification tokens, sorted. */
	google_verification: string[];
	og_type: string | null;
	noindex: boolean;
	nofollow: boolean;
}

export interface WebsiteProbe {
	requested_url: string;
	final_url: string;
	status: number;
	response_ms: number;
	redirects: WebsiteHop[];
	platform: string;
	/** The `category` option, else a guess (`ecommerce`, `blog`, `app`, `showcase`); null when unclear. */
	category: string | null;
	content_type: string;
	text_chars: number | null;
	blank: boolean | null;
	expect_text_found: boolean | null;
	metadata: WebsiteMetadata | null;
}

export interface WebsiteField {
	field: string;
	value: string | null;
	pending: string | null;
	changed_at: string | null;
	recently_changed: boolean;
}

export interface WebsiteOverview {
	checked_at: string | null;
	error: string | null;
	probe: WebsiteProbe | null;
	platform_label: string | null;
	fields: WebsiteField[];
	/** Last comparison of the metadata with the reference (every `change_check_minutes`). */
	changes_checked_at: string | null;
	robots_checked_at: string | null;
	robots_blocks_all: boolean | null;
	sitemap_ok: boolean | null;
}

export interface WebsiteChange {
	id: number;
	detected_at: string;
	field: string;
	before: string;
	after: string;
}

// --- DNS blocklists (crates/server/src/api/blacklist.rs) -----------------------
// Mail server reputation (`mail_blacklist`) and IP range blocklist
// (`ip_blacklist`), rebuilt from the last `dumbmonit_blacklist_*` series. The
// lookup and delisting links come from the collector's catalogue.

export type BlacklistCheckState = 'listed' | 'clean' | 'unknown' | 'refused' | 'not_applicable';

export interface BlacklistList {
	id: string;
	name: string;
	zone: string;
	subject: 'ip' | 'domain';
	/** A listing there blocks a large share of mail (Spamhaus, Barracuda, SpamCop). */
	major: boolean;
	/** Added by the user: absent from the built-in catalogue. */
	custom: boolean;
	checked: number;
	listed: number;
	unknown: number;
}

export interface BlacklistCheck {
	list: string;
	list_name: string;
	major: boolean;
	/** The address or domain asked about. */
	subject: string;
	state: BlacklistCheckState;
	codes: string[];
	meaning: string | null;
	/** TXT reason of a listing, or why a list gave no answer. */
	reason: string | null;
	/** Spamhaus refused the question because the resolver is a public one (or forwards to one). */
	public_resolver_refused: boolean;
	/** Only for a listing. */
	lookup_url: string | null;
	delist_url: string | null;
}

export interface BlacklistAddress {
	address: string;
	/** `input`, `mx`, `domain` or `extra`. */
	source: string;
	mx: string | null;
	ptr: string | null;
	/** The reverse DNS name resolves back to the address; `null` without an answer. */
	fcrdns: boolean | null;
}

export interface BlacklistAuth {
	mx_hosts: number | null;
	spf_records: number | null;
	dmarc_records: number | null;
	dmarc_policy: string | null;
}

export interface BlacklistProviderLink {
	/** `postmaster`, `guidelines`, `contact`, `snds`, `delist_m365`, `hub`; an unknown kind shows its URL. */
	kind: string;
	url: string;
}

export interface BlacklistProvider {
	id: string;
	name: string;
	links: BlacklistProviderLink[];
}

export interface BlacklistOverview {
	kind: string;
	/** Unix seconds of the last probe; `null` before the first one. */
	sampled_at: number | null;
	scan_running: boolean;
	scan_age_seconds: number | null;
	scan_duration_seconds: number | null;
	addresses_checked: number | null;
	addresses_listed: number | null;
	listed_lists: number | null;
	listed_major_lists: number | null;
	unknown_checks: number | null;
	domain_listed: boolean | null;
	lists: BlacklistList[];
	/** Listings and unanswered checks first. */
	checks: BlacklistCheck[];
	addresses: BlacklistAddress[];
	auth: BlacklistAuth | null;
	/** Mailbox providers without a public blocklist; empty for an IP range. */
	providers: BlacklistProvider[];
}

// ---------------------------------------------------------------------------
// Geocoding (`crates/server/src/api/geocode.rs`)

/** A place, as the geocoding returned it (`GET /api/locations/geocode`). */
export interface WallPlace {
	name: string;
	/** Region and country, to tell two places of the same name apart. */
	region: string;
	country_code: string;
	latitude: number;
	longitude: number;
}
