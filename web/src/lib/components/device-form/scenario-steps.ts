/**
 * The steps of an HTTP scenario (`http_steps`), as the `steps` option stores
 * them: a JSON array read by `crates/collectors/src/uptime/http/scenario/model.rs`.
 * Parsing, serialising, reordering and the checks the form can make before the
 * server does, kept apart from Svelte so they are testable.
 *
 * Also assembles the "last run" of a scenario from its per-step series, for the
 * monitor page.
 */

export const MAX_STEPS = 10;
export const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const;
export const CHECK_TYPES = ['contains', 'not_contains', 'json'] as const;
export const SOURCES = ['json', 'header', 'regex'] as const;

export type CheckType = (typeof CHECK_TYPES)[number];
export type Source = (typeof SOURCES)[number];

export interface StepHeader {
	name: string;
	value: string;
}

export interface StepCheck {
	type: CheckType;
	/** JSON path, for `json` only. */
	path: string;
	/** The text to look for, or the expected JSON value. */
	value: string;
}

export interface StepExtraction {
	name: string;
	from: Source;
	/** JSON path, header name or regular expression. */
	expression: string;
}

export interface ScenarioStep {
	name: string;
	method: string;
	url: string;
	headers: StepHeader[];
	body: string;
	/** Accepted codes, `200-299,302`; empty means any 2xx. */
	expect_status: string;
	assertions: StepCheck[];
	extract: StepExtraction[];
}

export function emptyStep(): ScenarioStep {
	return { name: '', method: 'GET', url: '', headers: [], body: '', expect_status: '', assertions: [], extract: [] };
}

export function emptyCheck(): StepCheck {
	return { type: 'contains', path: '', value: '' };
}

export function emptyExtraction(): StepExtraction {
	return { name: '', from: 'json', expression: '' };
}

const text = (value: unknown): string => (typeof value === 'string' ? value : '');
const list = (value: unknown): Record<string, unknown>[] =>
	Array.isArray(value) ? value.filter((entry): entry is Record<string, unknown> => typeof entry === 'object' && entry !== null) : [];

/** The steps of a tag; anything that is not a JSON array of objects gives none. */
export function parseSteps(raw: string | undefined): ScenarioStep[] {
	const source = (raw ?? '').trim();
	if (!source) return [];
	let parsed: unknown;
	try {
		parsed = JSON.parse(source);
	} catch {
		return [];
	}
	return list(parsed).map((entry) => ({
		name: text(entry.name),
		method: text(entry.method).toUpperCase() || 'GET',
		url: text(entry.url),
		headers: list(entry.headers).map((header) => ({ name: text(header.name), value: text(header.value) })),
		body: text(entry.body),
		expect_status: text(entry.expect_status),
		assertions: list(entry.assertions).map((check) => ({
			type: (CHECK_TYPES as readonly string[]).includes(text(check.type)) ? (text(check.type) as CheckType) : 'contains',
			path: text(check.path),
			value: text(check.value)
		})),
		extract: list(entry.extract).map((extraction) => ({
			name: text(extraction.name),
			from: (SOURCES as readonly string[]).includes(text(extraction.from)) ? (text(extraction.from) as Source) : 'json',
			expression: text(extraction.expression)
		}))
	}));
}

/**
 * The tag for these steps: defaults and blank lines left out (the server
 * applies the same defaults), empty when there is no step.
 */
export function serializeSteps(steps: ScenarioStep[]): string {
	if (steps.length === 0) return '';
	const out = steps.map((step) => {
		const entry: Record<string, unknown> = {};
		if (step.name.trim()) entry.name = step.name.trim();
		if (step.method && step.method !== 'GET') entry.method = step.method;
		entry.url = step.url.trim();
		const headers = step.headers.filter((header) => header.name.trim() || header.value.trim());
		if (headers.length > 0) entry.headers = headers.map((header) => ({ name: header.name.trim(), value: header.value }));
		if (step.body) entry.body = step.body;
		if (step.expect_status.trim()) entry.expect_status = step.expect_status.trim();
		const checks = step.assertions.filter((check) => check.value.trim() || check.path.trim());
		if (checks.length > 0) {
			entry.assertions = checks.map((check) =>
				check.type === 'json' ? { type: check.type, path: check.path.trim(), value: check.value } : { type: check.type, value: check.value }
			);
		}
		const extractions = step.extract.filter((extraction) => extraction.name.trim() || extraction.expression.trim());
		if (extractions.length > 0) {
			entry.extract = extractions.map((extraction) => ({
				name: extraction.name.trim(),
				from: extraction.from,
				expression: extraction.expression.trim()
			}));
		}
		return entry;
	});
	return JSON.stringify(out);
}

/** The list with the step at `from` moved to `to`; unchanged when either is out of range. */
export function moveStep<T>(steps: T[], from: number, to: number): T[] {
	if (from === to || from < 0 || to < 0 || from >= steps.length || to >= steps.length) return steps;
	const next = [...steps];
	const [moved] = next.splice(from, 1);
	next.splice(to, 0, moved);
	return next;
}

/** A variable name the server accepts: letters, digits and `_`, not starting with a digit. */
export function validVariableName(name: string): boolean {
	return /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(name) && name !== 'secret';
}

/** The `{{name}}` placeholders a text uses, secrets (`{{secret.…}}`) left out. */
export function usedVariables(source: string): string[] {
	const names: string[] = [];
	for (const match of source.matchAll(/\{\{\s*([^}|]*?)\s*(?:\|[^}]*)?\}\}/g)) {
		const name = match[1];
		if (!name.startsWith('secret.') && !names.includes(name)) names.push(name);
	}
	return names;
}

/** Something the form can tell before the server does. */
export type StepIssue =
	| { kind: 'url_missing' }
	| { kind: 'bad_name'; name: string }
	| { kind: 'unknown_variable'; name: string };

/**
 * Issues per step index (missing URL, invalid extraction name, a variable used
 * before any earlier step extracts it). Steps without issue are absent.
 */
export function stepIssues(steps: ScenarioStep[]): Map<number, StepIssue[]> {
	const issues = new Map<number, StepIssue[]>();
	const known = new Set<string>();
	steps.forEach((step, index) => {
		const found: StepIssue[] = [];
		if (!step.url.trim()) found.push({ kind: 'url_missing' });
		const texts = [step.url, step.body, ...step.headers.map((header) => header.value)];
		for (const name of texts.flatMap(usedVariables)) {
			if (!known.has(name) && !found.some((issue) => issue.kind === 'unknown_variable' && issue.name === name)) {
				found.push({ kind: 'unknown_variable', name });
			}
		}
		for (const extraction of step.extract) {
			const name = extraction.name.trim();
			if (!name && !extraction.expression.trim()) continue;
			if (!validVariableName(name)) found.push({ kind: 'bad_name', name });
			else known.add(name);
		}
		if (found.length > 0) issues.set(index, found);
	});
	return issues;
}

// --- Last run (monitor page) -------------------------------------------------

/** Values of `dumbmonit_probe_step_result`. */
export type StepResult = 'passed' | 'failed' | 'skipped';

export interface StepRun {
	/** Position, from 1. */
	index: number;
	/** The step's name, else its method and URL, else empty (the page then says "Step N"). */
	title: string;
	result: StepResult;
	/** HTTP status of this run, `null` when none came back. */
	status: number | null;
	/** Seconds, `null` for a step that did not run. */
	seconds: number | null;
}

/** One instant value per step: the value, and the time of the sample it comes from. */
export interface StepSample {
	step: number;
	value: number;
	/** Timestamp of the sample (`tlast_over_time`), in seconds. */
	at: number;
}

const RESULT: Record<number, StepResult> = { 1: 'passed', 0: 'failed', 2: 'skipped' };

/**
 * The steps of the latest run. Every series of a run carries the same
 * timestamp, so the run is the newest timestamp of the results; a step whose
 * result is older belongs to a previous version of the scenario and is left
 * out. A status counts only when it comes from that same run (a step that
 * failed before any answer has none); a duration is written by every step
 * that ran, so the latest one is that run's.
 */
export function lastRun(
	results: StepSample[],
	statuses: StepSample[],
	durations: StepSample[],
	steps: Pick<ScenarioStep, 'name' | 'method' | 'url'>[]
): StepRun[] {
	if (results.length === 0) return [];
	const run = Math.max(...results.map((sample) => sample.at));
	const sameRun = (sample: StepSample) => Math.abs(sample.at - run) < 1;
	return results
		.filter((sample) => sameRun(sample) && RESULT[sample.value] !== undefined)
		.sort((a, b) => a.step - b.step)
		.map((sample) => {
			const step = steps[sample.step - 1];
			const url = step?.url.trim() ?? '';
			const result = RESULT[sample.value];
			return {
				index: sample.step,
				title: step?.name.trim() || (url ? `${step?.method || 'GET'} ${url}` : ''),
				result,
				status: statuses.find((other) => other.step === sample.step && sameRun(other))?.value ?? null,
				seconds: result === 'skipped' ? null : (durations.find((other) => other.step === sample.step)?.value ?? null)
			};
		});
}
