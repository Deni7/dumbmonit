import type { CredentialField, CredentialFieldInput, CredentialView } from './targets.js';
import { CREDENTIAL_KINDS } from './targets.js';

// --- Network discovery ------------------------------------------------------

/**
 * Device detected by an SNMP scan, as the UI works with it.
 *
 * The server (`api/discovery.rs`) answers `{ devices, scanned }` with
 * `sysname`, `sysdescr`, `sysobjectid` and `suggested_profile`; `discover()`
 * folds that into this shape. `already_added` is computed client-side.
 */
export interface DiscoveredDevice {
	address: string;
	name: string | null;
	sysname: string | null;
	description: string | null;
	sysobjectid: string | null;
	kind: string | null;
	profile_id: string | null;
	already_added: boolean;
}

/** Result of a scan: the devices found and how many addresses were probed. */
export interface DiscoveryResult {
	devices: DiscoveredDevice[];
	scanned: number | null;
}

// --- Device types (collectors) ----------------------------------------------

/**
 * Setup notes for a device type.
 *
 * They describe what must be done **on the device itself** before it can be
 * monitored: enable SNMP, create an API token... `warning` and `doc_url` may be
 * empty, in which case nothing is displayed.
 */
export interface CollectorPort {
	port: number;
	protocol: string;
	/** `to_device`: DumbMonit (or its relay agent) to the device; `to_server`: agent to DumbMonit. */
	direction: string;
	purpose: string;
	/** Only used by an option or mode, named in `purpose`. */
	optional: boolean;
}

export interface CollectorSetup {
	title: string;
	steps: string[];
	/**
	 * Rights, roles or permissions to give the monitoring account, one per entry:
	 * the first line is the right (shown in bold), the optional second line says
	 * what it is for. The server omits the field when a type needs none.
	 */
	permissions?: string[];
	/** Network ports to open; the server omits the field when there are none. */
	ports?: CollectorPort[];
	warning: string;
	doc_url: string;
}

/**
 * Something the server's deployment must provide for a device type to work
 * fully (not something to do on the device): ICMP rights.
 *
 * The server only sends the identifier, whether it found the requirement in
 * place and the exact YAML to copy; the sentences are written by the interface
 * from `id` (see `RequirementNotice.svelte`), and an unknown `id` still renders.
 */
export interface CollectorRequirement {
	/** Stable identifier: `icmp`, or one a later server adds. */
	id: string;
	/** True when the server found the requirement met; the notice is then hidden. */
	satisfied: boolean;
	/** Documentation section that explains it. */
	doc_url: string;
	/** YAML to add to docker-compose.yml, as is. */
	compose_snippet: string;
}

/** Input shapes a collector option can ask for. */
/** `steps`: the requests of an HTTP scenario, a JSON array edited by `StepsEditor`. */
export type CollectorOptionInput = 'text' | 'number' | 'boolean' | 'select' | 'checklist' | 'rows' | 'agent' | 'steps';

/** One tick box of a `checklist` option. */
export interface CollectorOptionItem {
	/** What the tag stores. */
	value: string;
	label: string;
	/** One sentence saying what the entry is for. */
	help: string;
	/** Short mentions shown beside the name (type, importance…). */
	badges: string[];
}

/** One column of a `rows` option. */
export interface CollectorOptionColumn {
	key: string;
	label: string;
	placeholder: string;
	input: 'text' | 'select' | 'boolean';
	/** Values offered when `input === 'select'`; the first is the default. */
	choices: string[];
	/** JavaScript regular expression the text must match; empty: none. */
	pattern: string;
	/** Shown when `pattern` does not match. */
	invalid: string;
	/** Two rows cannot hold the same value here. */
	unique: boolean;
}

/**
 * Setting specific to a device type, described by the server.
 *
 * Each option is stored in `Target.tags[key]`, always as a string (`"true"` /
 * `"false"` for a boolean): the target model has no dedicated field, and tags
 * already travel with the target. The form uses it to display a typed field
 * rather than a key / value pair to guess.
 */
export interface CollectorOption {
	/** Key of the tag that carries the value. */
	key: string;
	label: string;
	help: string;
	placeholder: string;
	/** Value applied by the server when the tag is absent. */
	default: string;
	required: boolean;
	input: CollectorOptionInput;
	/** Values offered when `input === 'select'`. */
	choices: string[];
	/** Sub-heading grouping this setting under "More options". Empty: the UI falls back to a generic key-prefix heuristic. */
	group: string;
	/**
	 * Tick boxes when `input === 'checklist'`. The tag holds the ticked values
	 * separated by commas, `-` when none is ticked; absent means all of them.
	 * `default` is what a new device ticks.
	 */
	items: CollectorOptionItem[];
	/** Columns when `input === 'rows'`. The tag holds a JSON array of `{column key: value}`. */
	columns: CollectorOptionColumn[];
}

/** Nature of a metric family: a counter only grows and is charted as a rate. */
export type MetricKindName = 'gauge' | 'counter' | 'info' | 'state';

/** What one code of a `state` metric means. */
export interface MetricState {
	value: number;
	label: string;
	/** `ok`, `warning`, `critical` or `neutral`. */
	tone: string;
}

/** A metric family a device type emits, as the catalogue describes it. */
export interface MetricDescription {
	/** Name as the collector emits it, without the `dumbmonit_` prefix the server adds. */
	name: string;
	label: string;
	/** `%`, `s`, `B`, `B/s`, `°C`, `bool`…; for a counter, the unit of the running total. */
	unit: string;
	kind: MetricKindName;
	/** Which way a good figure goes; empty when neither. */
	better: '' | 'higher' | 'lower';
	min: number | null;
	max: number | null;
	/** Family giving the maximum of each series. */
	max_from: string;
	/** Label that names the measured object (`ifname`, `disk`…). */
	entity: string;
	states: MetricState[];
}

/** A view the catalogue recommends for the device page. */
export interface ViewDescription {
	/** `stat`, `timeseries`, `topn`, `heatmap`, or one a later server adds (drawn as curves). */
	view: string;
	title: string;
	/** Families shown, by `MetricDescription.name`. */
	metrics: string[];
	/** Label that tells entities apart; empty: every series. */
	group_by: string;
	options: Record<string, string>;
}

/**
 * A device type offered by this server.
 *
 * The list is built entirely from the server response: a later version that
 * adds a type sees it appear without any change here. A type unknown to the
 * interface therefore stays usable, with a neutral presentation (see
 * `normalizeCollector`).
 */
export interface CollectorInfo {
	kind: string;
	label: string;
	summary: string;
	examples: string[];
	credential_types: string[];
	/** The same families as `credential_types`, field by field, in order. */
	credentials: CredentialView[];
	address_hint: string;
	default_port: number;
	setup: CollectorSetup;
	/** Settings specific to this type. Empty for types that have none. */
	options: CollectorOption[];
	/** Server-side prerequisites of this type; the server omits the field when there are none. */
	requirements: CollectorRequirement[];
	/** Metric families this type declares; empty for a type (or a server) that declares none. */
	metrics: MetricDescription[];
	/** Recommended device-page views; empty: the page charts every family, as before. */
	views: ViewDescription[];
	/** Beta integration, still being tested on real installations. */
	beta: boolean;
}

/**
 * Fills in a collector description so that it is always displayable.
 *
 * The server returns empty fields for a type it cannot describe (`label` then
 * being the raw `kind` value). The interface must not display holes for all
 * that: we fill them here, once and for all, rather than multiplying `?.` and
 * `|| ''` in components.
 */
export function normalizeCollector(raw: Partial<CollectorInfo> & { kind: string }): CollectorInfo {
	const setup = raw.setup ?? { title: '', steps: [], permissions: [], ports: [], warning: '', doc_url: '' };
	const options = Array.isArray(raw.options) ? raw.options : [];
	const credential_types = Array.isArray(raw.credential_types)
		? raw.credential_types.filter((t) => typeof t === 'string')
		: [];
	// A server predating field descriptions only names the families: rebuild
	// the fields it would have sent, so the form looks the same against both.
	const credentials = Array.isArray(raw.credentials)
		? raw.credentials
				.filter((c) => c && typeof c.kind === 'string' && c.kind.trim())
				.map(normalizeCredentialView)
		: credential_types.map(fallbackCredentialView);
	return {
		kind: raw.kind,
		label: raw.label?.trim() || raw.kind,
		summary: raw.summary?.trim() ?? '',
		examples: Array.isArray(raw.examples) ? raw.examples : [],
		credential_types,
		credentials,
		address_hint: raw.address_hint?.trim() ?? '',
		default_port: typeof raw.default_port === 'number' ? raw.default_port : 0,
		setup: {
			title: setup.title?.trim() ?? '',
			steps: Array.isArray(setup.steps) ? setup.steps.filter((s) => s.trim()) : [],
			ports: Array.isArray(setup.ports)
				? setup.ports.filter((p) => p && Number.isInteger(p.port)).map((p) => ({
						port: p.port,
						protocol: (p.protocol ?? 'tcp').toLowerCase(),
						direction: p.direction === 'to_server' ? 'to_server' : 'to_device',
						purpose: p.purpose?.trim() ?? '',
						optional: p.optional === true
					}))
				: [],
			permissions: Array.isArray(setup.permissions) ? setup.permissions.filter((s) => s.trim()) : [],
			warning: setup.warning?.trim() ?? '',
			doc_url: setup.doc_url?.trim() ?? ''
		},
		options: options
			.filter((option) => option && typeof option.key === 'string' && option.key.trim())
			.map(normalizeOption),
		beta: raw.beta === true,
		requirements: Array.isArray(raw.requirements)
			? raw.requirements
					.filter((r) => r && typeof r.id === 'string' && r.id.trim())
					.map((r) => ({
						id: r.id.trim(),
						satisfied: r.satisfied === true,
						doc_url: r.doc_url?.trim() ?? '',
						compose_snippet: r.compose_snippet?.trim() ?? ''
					}))
			: [],
		metrics: Array.isArray(raw.metrics)
			? raw.metrics.filter((d) => d && typeof d.name === 'string' && d.name.trim()).map(normalizeMetric)
			: [],
		views: Array.isArray(raw.views)
			? raw.views
					.filter((v) => v && typeof v.view === 'string' && Array.isArray(v.metrics))
					.map((v) => ({
						view: v.view,
						title: typeof v.title === 'string' ? v.title : '',
						metrics: v.metrics.filter((n) => typeof n === 'string'),
						group_by: typeof v.group_by === 'string' ? v.group_by : '',
						options: v.options && typeof v.options === 'object' ? v.options : {}
					}))
			: []
	};
}

const METRIC_KINDS: readonly MetricKindName[] = ['gauge', 'counter', 'info', 'state'];

/** Fills the optional fields of a catalogue metric (the server omits empty ones). */
function normalizeMetric(raw: Partial<MetricDescription> & { name: string }): MetricDescription {
	const kind = METRIC_KINDS.find((k) => k === raw.kind) ?? 'gauge';
	return {
		name: raw.name.trim(),
		label: raw.label?.trim() ?? '',
		unit: raw.unit?.trim() ?? '',
		kind,
		better: raw.better === 'higher' || raw.better === 'lower' ? raw.better : '',
		min: typeof raw.min === 'number' ? raw.min : null,
		max: typeof raw.max === 'number' ? raw.max : null,
		max_from: raw.max_from?.trim() ?? '',
		entity: raw.entity?.trim() ?? '',
		states: Array.isArray(raw.states)
			? raw.states
					.filter((s) => s && typeof s.value === 'number' && typeof s.label === 'string')
					.map((s) => ({ value: s.value, label: s.label, tone: typeof s.tone === 'string' ? s.tone : 'neutral' }))
			: []
	};
}

const OPTION_INPUTS: readonly CollectorOptionInput[] = ['text', 'number', 'boolean', 'select', 'checklist', 'rows', 'agent'];

/**
 * Fills in an option so that it is always displayable.
 *
 * An older server may omit fields, or announce an input shape this interface
 * does not know: we then fall back to a text field, which accepts anything the
 * server will be able to read.
 */
function normalizeOption(raw: Partial<CollectorOption> & { key: string }): CollectorOption {
	const items: CollectorOptionItem[] = Array.isArray(raw.items)
		? raw.items
				.filter((item) => item && typeof item.value === 'string' && item.value.trim())
				.map((item) => ({
					value: item.value.trim(),
					label: item.label?.trim() || item.value.trim(),
					help: item.help?.trim() ?? '',
					badges: Array.isArray(item.badges) ? item.badges.filter((b) => typeof b === 'string' && b.trim()) : []
				}))
		: [];
	const columns: CollectorOptionColumn[] = Array.isArray(raw.columns)
		? raw.columns
				.filter((column) => column && typeof column.key === 'string' && column.key.trim())
				.map((column) => ({
					key: column.key.trim(),
					label: column.label?.trim() || column.key.trim(),
					placeholder: column.placeholder?.trim() ?? '',
					input: column.input === 'select' || column.input === 'boolean' ? column.input : 'text',
					choices: Array.isArray(column.choices) ? column.choices.filter((c) => typeof c === 'string') : [],
					pattern: column.pattern?.trim() ?? '',
					invalid: column.invalid?.trim() ?? '',
					unique: Boolean(column.unique)
				}))
		: [];
	let input = OPTION_INPUTS.includes(raw.input as CollectorOptionInput) ? (raw.input as CollectorOptionInput) : 'text';
	// A checklist with nothing to tick, or rows without columns, cannot be shown: plain text instead.
	if ((input === 'checklist' && items.length === 0) || (input === 'rows' && columns.length === 0)) input = 'text';
	return {
		key: raw.key.trim(),
		label: raw.label?.trim() || raw.key.trim(),
		help: raw.help?.trim() ?? '',
		placeholder: raw.placeholder?.trim() ?? '',
		default: typeof raw.default === 'string' ? raw.default : '',
		required: Boolean(raw.required),
		input,
		choices: Array.isArray(raw.choices) ? raw.choices.filter((c) => typeof c === 'string') : [],
		group: raw.group?.trim() ?? '',
		items,
		columns
	};
}

const CREDENTIAL_INPUTS: readonly CredentialFieldInput[] = ['text', 'password', 'select'];

function normalizeCredentialView(raw: Partial<CredentialView> & { kind: string }): CredentialView {
	const fields = Array.isArray(raw.fields) ? raw.fields : [];
	return {
		kind: raw.kind.trim(),
		label: raw.label?.trim() || fallbackCredentialView(raw.kind.trim()).label,
		help: raw.help?.trim() ?? '',
		fields: fields
			.filter((field) => field && typeof field.key === 'string' && field.key.trim())
			.map((field) => {
				const input = CREDENTIAL_INPUTS.includes(field.input as CredentialFieldInput)
					? (field.input as CredentialFieldInput)
					: 'text';
				return {
					key: field.key.trim(),
					label: field.label?.trim() || field.key.trim(),
					help: field.help?.trim() ?? '',
					placeholder: field.placeholder?.trim() ?? '',
					input,
					choices: Array.isArray(field.choices) ? field.choices.filter((c) => typeof c === 'string') : [],
					required: Boolean(field.required)
				};
			})
	};
}

function credentialField(
	key: string,
	label: string,
	input: CredentialFieldInput,
	required: boolean,
	extra: Partial<CredentialField> = {}
): CredentialField {
	return { key, label, help: '', placeholder: '', input, choices: [], required, ...extra };
}

/** The fields a family asks for, when the server does not say (older builds). */
function fallbackCredentialView(kind: string): CredentialView {
	const label = CREDENTIAL_KINDS.find((option) => option.value === kind)?.label ?? kind;
	switch (kind) {
		case 'snmp_community':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('community', 'SNMP community', 'password', true, {
						help: 'Most devices ship with "public". A read-only community is enough.',
						placeholder: 'public'
					})
				]
			};
		case 'snmp_v3':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('username', 'User name', 'text', true),
					credentialField('auth_protocol', 'Authentication protocol', 'select', false, {
						choices: ['sha256', 'sha512', 'sha384', 'sha224', 'sha1', 'md5']
					}),
					credentialField('auth_passphrase', 'Authentication passphrase', 'password', true),
					credentialField('privacy_protocol', 'Privacy protocol', 'select', false, {
						choices: ['aes128', 'aes192', 'aes256', 'des']
					}),
					credentialField('privacy_passphrase', 'Privacy passphrase', 'password', false, {
						help: 'Leave blank for authentication without encryption (authNoPriv).'
					})
				]
			};
		case 'api_token':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('token', 'API token', 'password', true, {
						help: 'A read-only token is enough. The server stores it encrypted and never shows it again.'
					})
				]
			};
		case 'username_password':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('username', 'User name', 'text', true),
					credentialField('password', 'Password', 'password', true)
				]
			};
		case 'radius':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('shared_secret', 'Shared secret', 'password', true),
					credentialField('username', 'Test account', 'text', true),
					credentialField('password', 'Test account password', 'password', true)
				]
			};
		case 'ssh_login':
			return {
				kind,
				label,
				help: '',
				fields: [
					credentialField('username', 'User name', 'text', true),
					credentialField('password', 'Password', 'password', true),
					credentialField('enable_secret', 'Enable password', 'password', false)
				]
			};
		default:
			return { kind, label, help: '', fields: [] };
	}
}
