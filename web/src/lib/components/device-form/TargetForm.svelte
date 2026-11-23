<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * The device form, shared by "Add a device" and "Edit".
	 *
	 * The kind is decided before this form exists (picker on add, fixed on
	 * edit), so it only shows what that kind asks for: name, address, the
	 * credential families it accepts, its first settings. Everything else waits
	 * behind "More options". The form is uncontrolled: fields are seeded once
	 * from `target` and the pages re-mount it (`{#key}`) when the subject changes.
	 */
	import { untrack } from 'svelte';
	import { ChevronDown, PlugZap } from 'lucide-svelte';
	import {
		createTarget,
		deleteRuleOverride,
		listRuleOverrides,
		putRuleOverride,
		testTarget,
		updateTarget,
		type TargetTestReport,
		type CollectorInfo,
		type CredentialView,
		type Target,
		type TargetPayload,
		type TargetRule
	} from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { knownFolders } from '#lib/components/devices/folders.js';
	import { guessAgent } from '#lib/components/devices/mdaemon/agent-link.js';
	import { groupOptions } from './option-groups';
	import { listRelayKinds, listRelays, relayable } from '#lib/api/relay.js';
	import { listLocations } from '#lib/api/locations.js';
	import type { MapLocation } from '#lib/api/types.js';
	import { isMultisiteKind } from '#lib/format.js';
	import type { RelayAgent } from '#lib/api/types.js';
	import { Button, ClickSpark, DocLink, ErrorNotice, Field, Toggle } from '#lib/ui/index.js';
	import { kindDocPage } from '#lib/docs.js';
	import CredentialFields from './CredentialFields.svelte';
	import AgentLink from './AgentLink.svelte';
	import OptionsFields from './OptionsFields.svelte';
	import { firstRowError, parseRows, rowErrors, seedChecklists } from './option-values';
	import { parseSteps, stepIssues } from './scenario-steps';
	import TagsEditor from './TagsEditor.svelte';
	import ChannelsField from './ChannelsField.svelte';
	import { sameChannels, templateSeed } from './template-seed';
	import type { MonitorTemplate } from '#lib/api/monitorTemplates.js';
	import RequirementNotice from './RequirementNotice.svelte';
	import NameAddressFields from './NameAddressFields.svelte';
	import IntervalField from './IntervalField.svelte';
	import ReachFields from './ReachFields.svelte';
	import TestResult from './TestResult.svelte';
	import MonitoredRules from './MonitoredRules.svelte';
	import { overrideActions, pendingChanges } from './monitored-rules';
	import { draftChanged, prefillDraft, secretPolicy, type SavedCredential } from './credential-edit';
	import {
		emptyDraft,
		initialView,
		kindFromLabel,
		toCredential,
		validateCredential,
		type CredentialDraft,
		type CredentialErrors
	} from './credentials';
	import { DEFAULT_INTERVAL, INTERVALS, SNMP_KIND, suggestName } from './kinds';
	import {
		MIN_INTERVAL_SECS,
		intervalProblem,
		intervalSeconds,
		splitInterval,
		type IntervalUnit
	} from './interval';
	import { withoutSecretTags } from './discovery-prefill';
	import { useDirtyTracker } from '#lib/settings/unsaved.svelte.js';
	import { firstReason, testBlocker } from './test-connection';
	import { seedFromPrefill, type Prefill } from './discovery-prefill';

	interface Props {
		collector: CollectorInfo;
		/** Present on edit, absent on add. */
		target?: Target;
		/**
		 * Add only: the monitor this one is copied from. Its settings seed the form
		 * (the name gets " (copy)"); no secret comes along, they are typed again.
		 */
		duplicateOf?: Target;
		/** Add only: the monitor template that seeds the form, as a copy would (no name, address or secret in it). */
		template?: MonitorTemplate;
		/** Candidates for "Parent device". The target itself is filtered out. */
		targets?: Target[];
		onsaved: (target: Target) => void;
		cancelHref: string;
		/** Add only: values found by the network scan (address, name, settings). Never a secret. */
		prefill?: Prefill | null;
		/** Add only: the relay agent picked beforehand ("Add a monitor through this relay"). */
		viaAgentDefault?: number | null;
		/** Location and folder a new device starts in (the site chosen before the form). */
		locationDefault?: number | null;
		folderDefault?: string;
	}

	let { collector, target, duplicateOf, template, targets = [], onsaved, cancelHref, prefill = null, viaAgentDefault = null, locationDefault = null, folderDefault = '' }: Props = $props();

	/** Value of the interval select that opens the number and unit fields. */
	const CUSTOM_INTERVAL = 'custom';

	// The scan's findings, folded into this kind's fields. Seeds once, like the rest.
	const seed = untrack(() => seedFromPrefill(target || duplicateOf || template ? null : prefill, collector));
	/** What the fields are seeded from: the edited monitor, the one being copied, or a template. */
	const base = untrack(
		() =>
			target ??
			duplicateOf ??
			(template ? templateSeed(template, { viaAgent: viaAgentDefault, location: locationDefault, folder: folderDefault }) : undefined)
	);

	const editing = $derived(target !== undefined);
	/** The credential families this kind accepts, field by field, from the server. */
	const views = $derived(collector.credentials);

	// --- Seed once from the target -------------------------------------------
	const initialCredential: CredentialView = untrack(() => initialView(collector.credentials, base?.credential_kind));
	/** The saved credential as the API describes it: kind, whether a secret is stored, its public parts. */
	const saved: SavedCredential | null = untrack(() =>
		target
			? {
					kind: kindFromLabel(target.credential_kind, ''),
					has_secret: target.has_secret,
					public: target.credential_public ?? {}
				}
			: null
	);
	/** On edit, the saved user name, token ID and protocols, secrets blank. */
	const initialDraft: CredentialDraft = untrack(() =>
		target
			? prefillDraft(initialCredential, saved)
			: duplicateOf
				? // The copy keeps the user name and token ID, never a secret.
					prefillDraft(initialCredential, {
						kind: kindFromLabel(duplicateOf.credential_kind, ''),
						has_secret: false,
						public: duplicateOf.credential_public ?? {}
					})
				: {
					...emptyDraft(initialCredential),
					// "public" is the factory community of nearly every device: pre-filled on add.
					...(initialCredential.kind === 'snmp_community' ? { community: 'public' } : {})
				}
	);

	let name = $state(untrack(() => (duplicateOf ? m.w1monitors_form_copy_name({ name: duplicateOf.name }) : (target?.name ?? seed.name))));
	let address = $state(untrack(() => base?.address ?? seed.address));
	// The interval: one of the presets, or a number and a unit ("Custom").
	const initialInterval = untrack(() => base?.interval_secs ?? DEFAULT_INTERVAL);
	const initialSplit = splitInterval(initialInterval);
	let intervalChoice = $state(INTERVALS.some((o) => o.value === initialInterval) ? String(initialInterval) : CUSTOM_INTERVAL);
	let customValue = $state(String(initialSplit.value));
	let customUnit = $state<IntervalUnit>(initialSplit.unit);
	/** Seconds between two checks; `null` while the custom value is not a number. */
	const intervalSecs = $derived(intervalChoice === CUSTOM_INTERVAL ? intervalSeconds(customValue, customUnit) : Number(intervalChoice));
	const intervalIssue = $derived(intervalProblem(intervalSecs));
	const intervalError = $derived(
		intervalIssue === 'min'
			? m.w1monitors_form_interval_err_min({ min: MIN_INTERVAL_SECS })
			: intervalIssue === 'max'
				? m.w1monitors_form_interval_err_max()
				: intervalIssue === 'invalid'
					? m.w1monitors_form_interval_err_invalid()
					: null
	);
	/** Free text for the team, shown on the monitor's page. */
	let note = $state(untrack(() => base?.note ?? ''));
	let parentId = $state<number | null>(untrack(() => base?.parent_id ?? null));
	let viaAgent = $state<number | null>(untrack(() => (base ? base.via_agent : viaAgentDefault) ?? null));
	// Probe from several locations (uptime kinds). Exclusive with `viaAgent`.
	let msOn = $state(untrack(() => !!base?.multisite));
	let msServer = $state(untrack(() => base?.multisite?.server ?? true));
	let msAgents = $state<number[]>(untrack(() => [...(base?.multisite?.agents ?? [])]));
	let msQuorum = $state<number | null>(untrack(() => base?.multisite?.quorum ?? null));
	let enabled = $state(untrack(() => base?.enabled ?? true));
	let cveScan = $state(untrack(() => base?.cve_scan ?? true));
	let groupName = $state(untrack(() => base?.group_name ?? folderDefault));
	/** The monitor's own notification channels; empty: every channel its rules choose. */
	let channels = $state<number[]>(untrack(() => [...(base?.channels ?? [])]));
	/** Sent only when changed (or chosen for a new monitor): an unreadable list never clears them. */
	const sendChannels = $derived(target ? !sameChannels(channels, target.channels ?? []) : channels.length > 0);
	// MSP mode: the client of the target. Offered when there is a choice; only
	// the MSP administrator moves an existing target (its dependents follow).
	const clientChoices = $derived(auth.mspOn ? (auth.msp?.clients ?? []) : []);
	const showClient = $derived(clientChoices.length > 1 || (auth.msp?.admin === true && clientChoices.length > 0));
	const clientLocked = $derived(!!target && auth.msp?.admin !== true);
	let clientId = $state<number | null>(untrack(() => target?.client_id ?? auth.currentClient ?? null));
	let locationId = $state<number | null>(untrack(() => base ? (base.location_id ?? null) : locationDefault));
	/** Locations of the world map; the field only shows once there is one. */
	let locations = $state<MapLocation[]>([]);
	$effect(() => {
		const controller = new AbortController();
		listLocations(controller.signal)
			.then((found) => (locations = found.locations))
			.catch(() => {
				// No locations to offer: the field stays hidden, the form still works.
			});
		return () => controller.abort();
	});
	const folderChoices = $derived(knownFolders(targets));
	// A new device ticks what the server describes as the default of each checklist
	// (the major blocklists, say); an existing one keeps what it has, and no tag
	// there means every box.
	let tags = $state<Record<string, string>>(
		untrack(() =>
			target
				? { ...target.tags }
				: duplicateOf
					? withoutSecretTags(duplicateOf.tags)
					: template
						? { ...template.tags }
						: seedChecklists(collector.options, seed.tags)
		)
	);
	let credential = $state<CredentialView>(initialCredential);
	let draft = $state<CredentialDraft>(untrack(() => ({ ...initialDraft })));

	function changeCredential(kind: string) {
		const next = views.find((view) => view.kind === kind);
		if (!next || next.kind === credential.kind) return;
		credential = next;
		// Back to the saved family: its saved parts come back too.
		draft = next.kind === initialCredential.kind ? { ...initialDraft } : prefillDraft(next, saved);
	}

	let touched = $state<Set<string>>(new Set());
	let attempted = $state(false);
	let saving = $state(false);
	let serverError = $state<unknown>(null);

	// Three settings up front; "More options" is open from the start.
	const PRIMARY_COUNT = 3;
	let more = $state(true);

	function touch(field: string) {
		if (!touched.has(field)) touched = new Set(touched).add(field);
	}

	// --- Options vs free tags -------------------------------------------------
	const options = $derived(collector.options);
	const optionKeys = $derived(options.map((o) => o.key));
	/** The first few settings sit in the main form; the rest behind "More options". */
	/** The agent chosen for a mail server has its own box at the top of the form. */
	const agentOption = $derived(options.find((o) => o.input === 'agent') ?? null);
	const listed = $derived(options.filter((o) => o.input !== 'agent'));
	const primaryOptions = $derived(listed.filter((o, i) => o.required || i < PRIMARY_COUNT));
	const moreOptions = $derived(listed.filter((o) => !primaryOptions.includes(o)));
	const moreOptionGroups = $derived(groupOptions(moreOptions));
	const optionValues = $derived(Object.fromEntries(Object.entries(tags).filter(([k]) => optionKeys.includes(k))));
	const freeTags = $derived(Object.fromEntries(Object.entries(tags).filter(([k]) => !optionKeys.includes(k))));

	function setOptions(values: Record<string, string>) {
		tags = { ...freeTags, ...values };
	}
	function setFreeTags(values: Record<string, string>) {
		tags = { ...values, ...optionValues };
	}

	// --- Agent installed on the server (mail servers) --------------------------
	let agentTouched = $state(false);
	let agentGuessed = $state(false);
	function setAgent(value: string) {
		const key = agentOption?.key;
		if (!key) return;
		const next = { ...tags };
		if (value) next[key] = value;
		else delete next[key];
		tags = next;
	}
	function chooseAgent(value: string) {
		agentTouched = true;
		agentGuessed = false;
		setAgent(value);
	}
	// A new monitor pre-selects the agent whose host name is the address typed,
	// until the user chooses by hand.
	$effect(() => {
		const option = agentOption;
		if (!option || target || agentTouched) return;
		const guess = guessAgent(address, targets);
		const next = guess ? String(guess.id) : '';
		untrack(() => {
			if ((tags[option.key] ?? '') !== next) setAgent(next);
			agentGuessed = next !== '';
		});
	});

	// --- Validation -----------------------------------------------------------
	const credentialChanged = $derived(credential.kind !== initialCredential.kind || draftChanged(draft, initialDraft));
	/**
	 * The server never sends a saved secret to a new destination: changing the
	 * address, the relay or how the device is reached (these settings) means
	 * typing the credential again. Mirrors `DESTINATION_TAGS` in `api/targets.rs`.
	 */
	const DESTINATION_TAGS = ['scheme', 'port', 'insecure_tls', 'tls', 'path', 'base_path', 'url', 'host', 'steps'];
	const destinationChanged = $derived.by(() => {
		if (!target || target.credential_kind === 'None') return false;
		const sentTag = (key: string) => (tags[key] ?? '').trim() || undefined;
		const savedTag = (key: string) => target.tags[key];
		return (
			address.trim() !== target.address.trim() ||
			(canRelay ? viaAgent : null) !== target.via_agent ||
			multisiteAgentsKey(canMultisite && msOn ? msAgents : []) !== multisiteAgentsKey(target.multisite?.agents ?? []) ||
			DESTINATION_TAGS.some((key) => sentTag(key) !== savedTag(key))
		);
	});
	/** On edit an untouched credential is simply not sent: the server keeps it. */
	const sendCredential = $derived(!editing || credentialChanged || destinationChanged);
	/**
	 * Which saved secrets a blank field keeps, and which must be typed again
	 * (the user name or token ID changed, or the destination did).
	 */
	const policy = $derived(secretPolicy(credential, draft, saved, destinationChanged));

	const nameError = $derived(name.trim() ? null : m.deviceform_form_name_required());
	const addressError = $derived(address.trim() ? null : m.deviceform_form_address_required());
	const credentialErrors = $derived<CredentialErrors>(
		sendCredential ? validateCredential(credential, draft, editing ? policy : null) : {}
	);
	const optionErrors = $derived.by(() => {
		const errors: Record<string, string> = {};
		for (const option of options) {
			if (option.input === 'rows') {
				const problem = firstRowError(
					rowErrors(option, parseRows(option, tags[option.key]), {
						invalid: m.deviceform_options_rows_invalid(),
						duplicate: m.deviceform_options_rows_duplicate()
					})
				);
				// Shown line by line under the rows; here it only keeps the form from being sent.
				if (problem) errors[option.key] = problem;
			}
			// Same for the steps of a scenario: each problem is said under its step.
			if (option.input === 'steps' && stepIssues(parseSteps(tags[option.key])).size > 0) {
				errors[option.key] = m.deviceform_steps_invalid();
			}
			if (!option.required) continue;
			if ((tags[option.key] ?? '').trim() || option.default.trim()) continue;
			errors[option.key] = m.deviceform_form_option_required({ label: option.label });
		}
		return errors;
	});

	const valid = $derived(
		!nameError && !addressError && !intervalError && Object.keys(credentialErrors).length === 0 && Object.keys(optionErrors).length === 0
	);

	function shown(field: string, error: string | null | undefined): string | undefined {
		return error && (attempted || touched.has(field)) ? error : undefined;
	}
	const shownCredentialErrors = $derived.by(() => {
		const out: CredentialErrors = {};
		for (const [field, message] of Object.entries(credentialErrors)) {
			if (attempted || touched.has(`cred.${field}`)) out[field] = message;
		}
		return out;
	});
	// Required settings without a default are rare (none today): show them at once.
	const shownOptionErrors = $derived(optionErrors);

	// --- Submit ---------------------------------------------------------------
	const addressHelp = $derived.by(() => {
		const parts: string[] = [];
		if (collector.address_hint) parts.push(m.deviceform_form_address_example({ example: collector.address_hint }));
		if (collector.default_port > 0) parts.push(m.deviceform_form_address_port({ port: collector.default_port }));
		return parts.join(' ') || undefined;
	});

	const parentCandidates = $derived(
		targets.filter((t) => t.id !== target?.id).sort((a, b) => a.name.localeCompare(b.name))
	);

	// --- Reached through (relay agents) --------------------------------------
	// Agents push their metrics, heartbeats are called, website checks run on the
	// server: only the kinds a relay knows are offered (`GET /relays/kinds`). A
	// relay already set on an edited monitor stays visible.
	let relayKinds = $state<string[] | null>(null);
	$effect(() => {
		const controller = new AbortController();
		listRelayKinds(controller.signal)
			.then((kinds) => (relayKinds = kinds))
			.catch(() => (relayKinds = null));
		return () => controller.abort();
	});
	const canRelay = $derived(relayable(collector.kind, relayKinds) || (target?.via_agent ?? null) !== null);
	let relays = $state<RelayAgent[]>([]);
	/** Set once the list came back, so the "no relay yet" hint never flashes before it. */
	let relaysRead = $state(false);
	$effect(() => {
		if (!canRelay) return;
		const controller = new AbortController();
		listRelays(controller.signal)
			.then((list) => {
				relays = list.filter((r) => r.id !== target?.id);
				relaysRead = true;
			})
			.catch(() => (relays = []));
		return () => controller.abort();
	});
	// --- Several locations -----------------------------------------------------
	const canMultisite = $derived(isMultisiteKind(collector.kind));
	const multisiteAgentsKey = (ids: number[]) => [...ids].sort((a, b) => a - b).join(',');
	const msTotal = $derived((msServer ? 1 : 0) + msAgents.length);
	const msTooFew = $derived(msOn && msTotal < 2);

	function buildPayload(): TargetPayload {
		const payload: TargetPayload = {
			name: name.trim(),
			address: address.trim(),
			kind: collector.kind,
			// The profile is detected by the server; on edit the detected one is kept.
			profile_id: target?.profile_id ?? null,
			parent_id: parentId,
			via_agent: canRelay && !(canMultisite && msOn) ? viaAgent : null,
			...(canMultisite && (msOn || target?.multisite)
				? { multisite: msOn ? { server: msServer, agents: msAgents, quorum: msQuorum } : null }
				: {}),
			interval_secs: intervalSecs ?? DEFAULT_INTERVAL,
			note: note.trim(),
			enabled,
			cve_scan: cveScan,
			group_name: groupName.trim(),
			location_id: locationId,
			...(sendChannels ? { channels } : {}),
			...(showClient && clientId !== null && !clientLocked ? { client_id: clientId } : {}),
			// No empty tag: a blank setting means "server default".
			tags: Object.fromEntries(Object.entries(tags).filter(([, v]) => v.trim()))
		};
		if (sendCredential) payload.credential = toCredential(credential, draft, editing ? policy.kept : []);
		return payload;
	}

	// Monitored alerts (edit only): switches moved, saved after the device.
	let ruleChoices = $state<Record<number, boolean>>({});
	let monitoredRules = $state<TargetRule[]>([]);
	let rulesError = $state<unknown>(null);

	// Leaving with edits not saved asks first (navigation and tab close alike).
	// The switches of the monitored alerts count too.
	const tracker = useDirtyTracker(() => ({
		name,
		address,
		intervalChoice,
		customValue,
		customUnit,
		note,
		parentId,
		viaAgent,
		msOn,
		msServer,
		msAgents,
		msQuorum,
		enabled,
		cveScan,
		groupName,
		channels,
		clientId,
		locationId,
		tags,
		credential: credential.kind,
		draft,
		rules: ruleChoices
	}));
	// The form starts clean: the baseline is taken once the seed and the first effects have run.
	$effect(() => {
		untrack(() => tracker.markClean());
	});

	/** Saves the moved switches as overrides; each one that lands is folded into the loaded state. */
	async function saveRuleChoices(targetId: number) {
		const changes = pendingChanges(monitoredRules, ruleChoices);
		if (changes.length === 0) return;
		const needsCurrent = auth.isAdmin && changes.some(({ rule }) => rule.custom_threshold);
		const current = needsCurrent ? await listRuleOverrides(targetId) : [];
		for (const action of overrideActions(changes, current, auth.isAdmin)) {
			if (action.method === 'PUT') await putRuleOverride(action.ruleId, targetId, action.payload);
			else await deleteRuleOverride(action.ruleId, targetId);
			const watched = ruleChoices[action.ruleId];
			monitoredRules = monitoredRules.map((rule) => (rule.id === action.ruleId ? { ...rule, ignored: !watched } : rule));
			const { [action.ruleId]: _, ...rest } = ruleChoices;
			ruleChoices = rest;
		}
	}

	// --- Test connection (SNMP) ----------------------------------------------
	const canTest = $derived(collector.kind === SNMP_KIND);
	const blocker = $derived(
		testBlocker({ address, credentialErrors, viaRelay: canRelay && viaAgent !== null })
	);
	const blockerText = $derived(
		blocker === 'address'
			? m.deviceform_test_need_address()
			: blocker === 'credential'
				? m.deviceform_test_need_credential()
				: blocker === 'relay'
					? m.deviceform_test_relay()
					: null
	);
	let testing = $state(false);
	let testReport = $state<TargetTestReport | null>(null);
	let testError = $state<unknown>(null);
	let testController: AbortController | null = null;

	// A result describes the settings it was run with: drop it when they change.
	$effect(() => {
		void [address, credential.kind, JSON.stringify(draft), JSON.stringify(tags), viaAgent, msOn, msServer, msAgents, msQuorum];
		untrack(() => {
			testController?.abort();
			testReport = null;
			testError = null;
		});
	});

	/** Resolves true only when the probe succeeded (the button then confirms in green). */
	async function runTest(): Promise<boolean> {
		if (blocker || testing) return false;
		testController?.abort();
		testController = new AbortController();
		const signal = testController.signal;
		testing = true;
		testReport = null;
		testError = null;
		try {
			const payload = buildPayload();
			// The server wants a name; the test does not need the person to pick one.
			payload.name = payload.name || suggestName(payload.address) || payload.address;
			testReport = await testTarget({ ...payload, id: target?.id }, signal);
			return testReport.ok;
		} catch (cause) {
			if (!(cause instanceof DOMException && cause.name === 'AbortError')) testError = cause;
			return false;
		} finally {
			testing = false;
		}
	}

	/** Why Save is disabled, in plain words (the button alone says nothing). */
	const submitBlocker = $derived(
		valid
			? null
			: firstReason([
					nameError,
					addressError,
					intervalError,
					...Object.values(credentialErrors),
					...Object.values(optionErrors)
				])
	);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		attempted = true;
		serverError = null;
		rulesError = null;
		if (!valid || saving || (canMultisite && msTooFew)) return;
		saving = true;
		try {
			const payload = buildPayload();
			const saved = target ? await updateTarget(target.id, payload) : await createTarget(payload);
			if (target) {
				try {
					await saveRuleChoices(saved.id);
				} catch (cause) {
					// The device is saved; keep the form open with the switches still moved.
					rulesError = cause;
					return;
				}
			}
			// Saved: the page may now navigate away without asking.
			tracker.markClean();
			onsaved(saved);
		} catch (cause) {
			serverError = cause;
		} finally {
			saving = false;
		}
	}
</script>

<form onsubmit={submit} class="grid gap-5" novalidate>
	{#if duplicateOf}
		<p class="rounded-[var(--radius-card)] border border-line bg-surface-2/60 px-3 py-2 text-sm text-ink-2" role="note">
			{m.w1monitors_form_copy_notice({ name: duplicateOf.name })}
		</p>
	{:else if template}
		<p class="rounded-[var(--radius-card)] border border-line bg-surface-2/60 px-3 py-2 text-sm text-ink-2" role="note">
			{m.w5monitors_form_template_notice({ name: template.name })}
		</p>
	{/if}
	<RequirementNotice requirements={collector.requirements} />
	{#if agentOption}
		<AgentLink {collector} option={agentOption} {targets} value={tags[agentOption.key] ?? ''} guessed={agentGuessed} onchange={chooseAgent} />
	{/if}
	<NameAddressFields {collector} bind:name bind:address {nameError} {addressError} {addressHelp} {shown} {touch} />

	{#if !(views.length <= 1 && credential.fields.length === 0)}
		<CredentialFields
			{views}
			selected={credential}
			bind:draft
			kept={editing ? policy.kept : []}
			reenter={editing ? Object.keys(policy.reenter) : []}
			errors={shownCredentialErrors}
			onkindchange={changeCredential}
			onblur={(field) => touch(`cred.${field}`)}
		/>
	{/if}

	{#if primaryOptions.length > 0}
		<OptionsFields options={primaryOptions} values={optionValues} errors={shownOptionErrors} onchange={setOptions} />
	{/if}

	<div class="graticule pb-3">
		<button
			type="button"
			class="inline-flex min-h-10 items-center gap-1.5 text-sm font-semibold text-ink-2 hover:text-ink sm:min-h-0"
			aria-expanded={more}
			aria-controls="target-more"
			onclick={() => (more = !more)}
		>
			<ChevronDown class={`size-4 transition-transform duration-200 ${more ? 'rotate-180' : ''}`} aria-hidden="true" />
			{m.deviceform_form_more_options()}
		</button>
	</div>

	{#if more}
		<div id="target-more" class="grid gap-5 rise-in">
			{#if moreOptions.length > 0}
				<div class="grid gap-4">
					<div class="grid gap-1">
						<p class="text-sm font-semibold text-ink">{m.deviceform_form_more_settings({ label: collector.label })}</p>
					</div>
					{#each moreOptionGroups as group (group.label || '_ungrouped')}
						<div class="grid gap-2">
							{#if group.label}
								<p class="text-[0.8125rem] font-semibold text-ink-2">{group.label}</p>
							{/if}
							<OptionsFields options={group.options} values={optionValues} errors={shownOptionErrors} onchange={setOptions} />
						</div>
					{/each}
				</div>
			{/if}

			<div class="grid gap-4 sm:grid-cols-2">
				<IntervalField bind:intervalChoice bind:customValue bind:customUnit {intervalSecs} {intervalError} {CUSTOM_INTERVAL} />

				{#if showClient}
					<Field label={m.deviceform_form_client_label()} for="target-client" help={target ? m.deviceform_form_client_help_edit() : m.deviceform_form_client_help()}>
						<select id="target-client" class="input" bind:value={clientId} disabled={clientLocked}>
							{#if clientId === null}<option value={null}>{m.deviceform_form_client_default()}</option>{/if}
							{#each clientChoices as choice (choice.id)}
								<option value={choice.id}>{choice.name}</option>
							{/each}
						</select>
					</Field>
				{/if}

				<Field label={m.deviceform_form_folder_label()} for="target-group" help={m.deviceform_form_folder_help()}>
					<input
						id="target-group"
						class="input"
						list="target-group-choices"
						maxlength="60"
						bind:value={groupName}
						placeholder={m.deviceform_form_folder_placeholder()}
					/>
					<datalist id="target-group-choices">
						{#each folderChoices as choice (choice)}<option value={choice}></option>{/each}
					</datalist>
				</Field>

				{#if locations.length > 0 || locationId !== null}
					<Field label={m.deviceform_form_location_label()} for="target-location" help={m.deviceform_form_location_help()}>
						<select id="target-location" class="input" bind:value={locationId}>
							<option value={null}>{m.deviceform_form_location_auto()}</option>
							{#each locations as location (location.id)}
								<option value={location.id}>{location.name}</option>
							{/each}
						</select>
					</Field>
				{/if}

				<Field
					label={m.deviceform_form_parent_label()}
					for="target-parent"
					help={m.deviceform_form_parent_help()}
				>
					<select id="target-parent" class="input" bind:value={parentId}>
						<option value={null}>{m.deviceform_form_parent_none()}</option>
						{#each parentCandidates as candidate (candidate.id)}
							<option value={candidate.id}>{candidate.name}</option>
						{/each}
					</select>
				</Field>

				<ReachFields {collector} {canRelay} {canMultisite} {relays} {relaysRead} {editing} bind:viaAgent bind:msOn bind:msServer bind:msAgents bind:msQuorum />
			</div>

			<TagsEditor tags={freeTags} reserved={optionKeys} onchange={setFreeTags} />

			<ChannelsField bind:value={channels} />

			<Field label={m.w1monitors_form_note_label()} for="target-note" help={m.w1monitors_form_note_help()}>
				<textarea
					id="target-note"
					class="input min-h-24 resize-y"
					rows="3"
					maxlength="2000"
					bind:value={note}
					placeholder={m.w1monitors_form_note_placeholder()}
				></textarea>
			</Field>

			<Field label={m.deviceform_form_enabled_label()} for="target-enabled" inline help={m.deviceform_form_enabled_help()}>
				<Toggle id="target-enabled" bind:checked={enabled} label={m.deviceform_form_enabled_label()} />
			</Field>

			<Field label={m.deviceform_form_cve_label()} for="target-cve-scan" inline help={m.deviceform_form_cve_help()}>
				<Toggle id="target-cve-scan" bind:checked={cveScan} label={m.deviceform_form_cve_label()} />
			</Field>
		</div>
	{/if}

	{#if target}
		<MonitoredRules targetId={target.id} bind:choices={ruleChoices} bind:rules={monitoredRules} />
	{/if}

	{#if rulesError}
		<ErrorNotice error={rulesError} title={m.deviceform_rules_save_error()} />
	{/if}

	{#if serverError}
		<ErrorNotice error={serverError} title={editing ? m.deviceform_form_save_error() : m.deviceform_form_add_error()} />
	{/if}

	<div class="flex flex-wrap items-center gap-2 pt-1">
		<ClickSpark>
			<Button
				type="submit"
				variant="primary"
				loading={saving}
				disabled={!valid}
				aria-describedby={submitBlocker ? 'target-submit-why' : undefined}
			>
				{editing ? m.deviceform_form_save() : m.deviceform_form_add()}
			</Button>
		</ClickSpark>
		{#if canTest}
			<Button
				type="button"
				variant="secondary"
				loading={testing}
				disabled={blocker !== null}
				onclick={runTest}
				aria-describedby={blockerText ? 'target-test-why' : undefined}
			>
				<PlugZap class="size-4" aria-hidden="true" />
				{testing ? m.deviceform_test_running() : m.deviceform_test_button()}
			</Button>
		{/if}
		<Button variant="ghost" href={cancelHref}>{m.deviceform_form_cancel()}</Button>
	</div>
	<DocLink page={kindDocPage(collector.kind)} />
	{#if submitBlocker}
		<p id="target-submit-why" class="text-[0.8125rem] text-ink-2">
			{m.deviceform_form_submit_blocked({ reason: submitBlocker })}
		</p>
	{/if}
	{#if canTest && blockerText}
		<p id="target-test-why" class="text-[0.8125rem] text-ink-2">{blockerText}</p>
	{/if}
	{#if canTest}
		<TestResult report={testReport} error={testError} />
	{/if}
</form>
