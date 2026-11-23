<script lang="ts">
	/**
	 * The rules behind the alerts. Each row can be toggled, and tuned in place:
	 * "Edit" unfolds an inline editor for the knobs that matter (threshold,
	 * hold, severity, channels, reminders) — never a query builder. A rule you
	 * added can be deleted; a shipped ("built-in") one cannot, and keeps its
	 * name. "New rule" adds a threshold rule with the minimum the server needs.
	 *
	 * The rules themselves belong to the page (it refreshes them every 30 s and
	 * after a toggle); a save is shown right away through a local overlay that
	 * the next refresh replaces.
	 */
	import type {
		AlertRule,
		AlertHistoryEntry,
		AlertRulePayload,
		Channel,
		Target,
		CollectorInfo
	} from '#lib/api/index.js';
	import { listChannels, resetAlertRule } from '#lib/api/index.js';
	import { page } from '$app/state';
	import { tick, untrack } from 'svelte';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import {
		Button,
		Confirm,
		Plate,
		Toggle,
		EmptyState,
		reducedMotion
	} from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { withoutOnCall } from '#lib/components/oncall/gate.js';
	import { instanceStore } from '#lib/stores/instance.svelte.js';
	import { SlidersHorizontal, ChevronDown, ChevronUp, Search } from 'lucide-svelte';
	import { formatDuration, formatRelative } from '#lib/format.js';
	import { severityRank, severityTone, severityWord } from './helpers';
	import { kindLabel as kindLabelOf } from '#lib/kind-label.js';
	import { m } from '#lib/paraglide/messages.js';
	import FileManagedBadge from '#lib/components/settings/FileManagedBadge.svelte';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import { ruleKinds } from './rules-filter';
	import Segmented from '#lib/components/devices/Segmented.svelte';
	import RuleEditor from './rules/RuleEditor.svelte';
	import NewRuleForm from './rules/NewRuleForm.svelte';
	import { anomalySummary, payloadFrom } from './rules/options';
	import { durationBase, isFlagUnit } from './rules/units';

	interface Props {
		rules: AlertRule[];
		/** Devices this instance actually has — decides which built-in rules are "relevant". */
		targets?: Target[];
		/** For kind labels, and for inferring which collector kind a rule's query is about. */
		collectors?: CollectorInfo[];
		/** Recent alert transitions, for the "fired recently" filter. */
		history?: AlertHistoryEntry[];
		busyId?: number | null;
		ontoggle: (rule: AlertRule, enabled: boolean) => void;
		ondelete: (id: number) => void;
		oncreate: (payload: AlertRulePayload) => Promise<void>;
	}

	let {
		rules,
		targets = [],
		collectors = [],
		history = [],
		busyId = null,
		ontoggle,
		ondelete,
		oncreate
	}: Props = $props();

	// --- Rows: overlay of saved rules until the page refreshes ---------------

	let saved = $state<Map<number, AlertRule>>(new Map());
	$effect(() => {
		// A fresh list from the page carries everything the overlay knew.
		void rules;
		saved = new Map();
	});
	const rows = $derived(rules.map((rule) => saved.get(rule.id) ?? rule));

	// --- Search, filters and groups --------------------------------------------
	//
	// ~50 collector kinds ship a handful of rules each: 150+ rules on one page.
	// They are grouped by device kind, every group folded. Groups for kinds
	// the instance owns come first. Typing, picking a kind or a quick filter
	// unfolds the groups that still match.

	type Quick = 'all' | 'active' | 'disabled' | 'changed' | 'fired';
	/** "Fired recently" looks back this far in the history. */
	const RECENT_MS = 7 * 24 * 3600 * 1000;

	let search = $state('');
	let kindFilter = $state('');
	let quick = $state<Quick>('all');
	let openGroups = $state<Set<string>>(new Set());

	const knownKinds = $derived(collectors.map((c) => c.kind));
	const ownedKinds = $derived(new Set(targets.map((t) => t.kind)));

	function kindLabel(kind: string): string {
		return kindLabelOf(kind, collectors.find((c) => c.kind === kind)?.label);
	}

	/** Kinds actually referenced by at least one rule — the select's options. */
	const kindOptions = $derived.by(() => {
		const seen = new Set<string>();
		for (const rule of rows) for (const kind of ruleKinds(rule, knownKinds)) seen.add(kind);
		return [...seen].map((kind) => [kind, kindLabel(kind)] as const).sort((a, b) => a[1].localeCompare(b[1], getLocale()));
	});

	/** Rule uids that went firing in the last days, from the history. */
	const recentlyFired = $derived.by(() => {
		const since = Date.now() - RECENT_MS;
		const uids = new Set<string>();
		for (const entry of history) {
			if (entry.to_phase !== 'firing') continue;
			// Timestamps are UTC; tolerate a missing suffix.
			const at = Date.parse(/[zZ]|[+-]\d\d:?\d\d$/.test(entry.at) ? entry.at : `${entry.at}Z`);
			if (at >= since) uids.add(entry.rule_uid);
		}
		for (const alert of alertsStore.alerts) {
			if (alert.effective_phase === 'firing') uids.add(alert.rule_uid);
		}
		return uids;
	});

	/** Added by the operator, or a shipped rule tuned for some device. */
	function isChanged(rule: AlertRule): boolean {
		return !rule.builtin || rule.overrides.length > 0;
	}

	function matchesQuick(rule: AlertRule, mode: Quick): boolean {
		switch (mode) {
			case 'active':
				return rule.enabled;
			case 'disabled':
				return !rule.enabled;
			case 'changed':
				return isChanged(rule);
			case 'fired':
				return recentlyFired.has(rule.uid);
			default:
				return true;
		}
	}

	/** Rules matching the search box and the kind select. */
	const searchResults = $derived.by(() => {
		const term = search.trim().toLowerCase();
		return rows.filter((rule) => {
			if (kindFilter && !ruleKinds(rule, knownKinds).includes(kindFilter)) return false;
			if (!term) return true;
			return (
				rule.name.toLowerCase().includes(term) ||
				rule.description.toLowerCase().includes(term) ||
				rule.query.toLowerCase().includes(term)
			);
		});
	});

	const quickOptions = $derived([
		{ id: 'all' as const, label: m.alerts_rules_quick_all(), count: searchResults.length },
		{ id: 'active' as const, label: m.alerts_rules_quick_active(), count: searchResults.filter((r) => r.enabled).length },
		{ id: 'disabled' as const, label: m.alerts_rules_quick_disabled(), count: searchResults.filter((r) => !r.enabled).length },
		{ id: 'changed' as const, label: m.alerts_rules_quick_changed(), count: searchResults.filter(isChanged).length },
		{ id: 'fired' as const, label: m.alerts_rules_quick_fired(), count: searchResults.filter((r) => recentlyFired.has(r.uid)).length }
	]);

	const filtered = $derived(search.trim() !== '' || kindFilter !== '' || quick !== 'all');

	type SortKey = 'default' | 'name' | 'severity' | 'fired';
	let sort = $state<SortKey>('default');
	const sortOptions = $derived([
		{ value: 'default' as const, label: m.w1alerts_rules_sort_default() },
		{ value: 'name' as const, label: m.w1alerts_rules_sort_name() },
		{ value: 'severity' as const, label: m.w1alerts_rules_sort_severity() },
		{ value: 'fired' as const, label: m.w1alerts_rules_sort_fired() }
	]);

	/** When the rule last fired, in ms; 0 when never. Live alerts count as now. */
	function firedAt(rule: AlertRule): number {
		if ((firingByRule.get(rule.uid) ?? 0) > 0) return Date.now();
		const at = Date.parse(rule.last_fired_at ? (/[zZ]|[+-]\d\d:?\d\d$/.test(rule.last_fired_at) ? rule.last_fired_at : `${rule.last_fired_at}Z`) : '');
		return Number.isNaN(at) ? 0 : at;
	}

	function sorted(list: AlertRule[]): AlertRule[] {
		if (sort === 'default') return list;
		const copy = [...list];
		if (sort === 'name') copy.sort((a, b) => a.name.localeCompare(b.name, getLocale()));
		else if (sort === 'severity') {
			copy.sort((a, b) => severityRank(a.severity) - severityRank(b.severity) || a.name.localeCompare(b.name, getLocale()));
		} else copy.sort((a, b) => firedAt(b) - firedAt(a) || a.name.localeCompare(b.name, getLocale()));
		return copy;
	}

	const shown = $derived(searchResults.filter((rule) => matchesQuick(rule, quick)));

	/** The group a rule is filed under: its device kind, or '' for universal rules. */
	function groupKindOf(rule: AlertRule): string {
		return [...ruleKinds(rule, knownKinds)].sort((a, b) => a.localeCompare(b, getLocale()))[0] ?? '';
	}

	/** Shown rules by device kind (universal ones under ''), owned kinds first. */
	const groups = $derived.by(() => {
		const byKind = new Map<string, AlertRule[]>();
		for (const rule of sorted(shown)) {
			const kind = groupKindOf(rule);
			const list = byKind.get(kind);
			if (list) list.push(rule);
			else byKind.set(kind, [rule]);
		}
		const rank = (kind: string) => (kind === '' ? 0 : ownedKinds.has(kind) ? 1 : 2);
		return [...byKind].sort(
			(a, b) => rank(a[0]) - rank(b[0]) || kindLabel(a[0]).localeCompare(kindLabel(b[0]), getLocale())
		);
	});

	function groupLabel(kind: string): string {
		return kind === '' ? m.alerts_rules_group_general() : kindLabel(kind);
	}

	function isOpen(kind: string): boolean {
		return filtered || openGroups.has(kind);
	}

	function setOpen(kind: string, open: boolean) {
		if (filtered || openGroups.has(kind) === open) return;
		const next = new Set(openGroups);
		if (open) next.add(kind);
		else next.delete(kind);
		openGroups = next;
	}

	// --- Opened from the history: `?rule=<uid>` unfolds, scrolls to and marks the rule ---

	let markedId = $state<number | null>(null);
	let consumedLink = '';
	$effect(() => {
		const uid = page.url.searchParams.get('rule');
		if (!uid || uid === consumedLink || rows.length === 0) return;
		const rule = untrack(() => rows.find((r) => r.uid === uid));
		if (!rule) return;
		consumedLink = uid;
		untrack(() => {
			// Filters would hide it: the link wins over them.
			search = '';
			kindFilter = '';
			quick = 'all';
			const kind = groupKindOf(rule);
			openGroups = new Set(openGroups).add(kind);
			markedId = rule.id;
			// From the noise report: `&edit=1` opens the editor, `&backtest=1` also runs the history test.
			const params = page.url.searchParams;
			if (auth.can('monitors.manage') && (params.get('edit') === '1' || params.get('backtest') === '1')) {
				editingId = rule.id;
				backtestId = params.get('backtest') === '1' ? rule.id : null;
			}
		});
		void tick().then(() => {
			document.getElementById(`rule-row-${rule.id}`)?.scrollIntoView({ block: 'center', behavior: reducedMotion() ? 'auto' : 'smooth' });
		});
		if (markTimer) clearTimeout(markTimer);
		markTimer = setTimeout(() => (markedId = null), 4000);
	});
	let markTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => () => {
		if (markTimer) clearTimeout(markTimer);
	});

	// --- Clone and reset ------------------------------------------------------

	let cloneNote = $state<string | null>(null);
	let actionError = $state<string | null>(null);
	let resettingId = $state<number | null>(null);

	/** A new rule with the same settings, under a "(copy)" name; shipped rules clone into your own. */
	async function clone(rule: AlertRule) {
		actionError = null;
		cloneNote = null;
		const copyName = m.w1alerts_rules_copy_name({ name: rule.name });
		try {
			await oncreate({ ...payloadFrom(rule), uid: undefined, name: copyName, enabled: false });
			cloneNote = m.w1alerts_rules_cloned({ name: copyName });
		} catch (cause) {
			actionError = cause instanceof Error ? cause.message : m.w1alerts_rules_error_clone();
		}
	}

	async function resetToDefaults(rule: AlertRule) {
		resettingId = rule.id;
		actionError = null;
		try {
			onSaved(await resetAlertRule(rule.id));
		} catch (cause) {
			actionError = cause instanceof Error ? cause.message : m.w1alerts_rules_error_reset();
		} finally {
			resettingId = null;
		}
	}

	let editingId = $state<number | null>(null);
	/** The rule whose editor opens straight on its history test (link from the noise report). */
	let backtestId = $state<number | null>(null);
	let shownQueryIds = $state<Set<number>>(new Set());
	let savedId = $state<number | null>(null);
	let savedTimer: ReturnType<typeof setTimeout> | null = null;

	function toggleQuery(id: number) {
		const next = new Set(shownQueryIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		shownQueryIds = next;
	}

	function onSaved(rule: AlertRule) {
		saved = new Map(saved).set(rule.id, rule);
		editingId = null;
		backtestId = null;
		savedId = rule.id;
		if (savedTimer) clearTimeout(savedTimer);
		savedTimer = setTimeout(() => (savedId = null), 4000);
	}
	$effect(() => () => {
		if (savedTimer) clearTimeout(savedTimer);
	});

	// --- Firing counters, from the shared store -------------------------------

	const firingByRule = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const alert of alertsStore.alerts) {
			if (alert.effective_phase !== 'firing') continue;
			counts.set(alert.rule_uid, (counts.get(alert.rule_uid) ?? 0) + 1);
		}
		return counts;
	});

	// --- Channels, loaded once for the editors --------------------------------

	let channels = $state<Channel[]>([]);
	let channelsError = $state<string | null>(null);
	// On-call channels are not offered while on-call schedules are off.
	const pickableChannels = $derived(withoutOnCall(channels, instanceStore.oncallEnabled));
	$effect(() => {
		const controller = new AbortController();
		listChannels(controller.signal)
			.then((list) => (channels = list))
			.catch((cause) => {
				if (cause instanceof DOMException && cause.name === 'AbortError') return;
				channelsError = cause instanceof Error ? cause.message : 'unknown error';
			});
		return () => controller.abort();
	});

	/** Devices where this rule has been told "don't alert me about this again". */
	function ignoredCount(rule: AlertRule): number {
		return rule.overrides.filter((o) => o.enabled === false).length;
	}

	/** "All enabled channels" / "Telegram, Email" for the row summary. */
	function channelsLabel(rule: AlertRule): string {
		if (rule.channels.length === 0) return m.alerts_rules_all_channels();
		const names = rule.channels.map(
			(id) => channels.find((c) => c.id === id)?.name ?? m.alerts_rules_channel_id({ id })
		);
		return names.join(', ');
	}

	/** One line of plain words: "above 90% for 10 min · every 6 h". */
	function summary(rule: AlertRule): string {
		const parts: string[] = [];
		if (rule.kind === 'anomaly') {
			parts.push(m.alerts_rules_baseline_anomaly());
		} else if (rule.kind === 'forecast') {
			parts.push(
				m.alerts_rules_forecast_summary({
					days: rule.threshold,
					warning: rule.params.forecast_warning_days ?? 14,
					critical: rule.params.forecast_critical_days ?? 7
				})
			);
		} else if (rule.kind === 'drift') {
			parts.push(m.alerts_rules_drift_summary({ ratio: rule.threshold }));
		} else if (rule.kind === 'slo_burn') {
			parts.push(m.alerts_rules_slo_burn_summary());
		} else if (isFlagUnit(rule.unit)) {
			// An on/off signal: "above 0" would say nothing.
			parts.push(m.w8rules_list_summary_flag());
		} else {
			// A duration reads as one: "above 7 d", not "above 604800 s".
			const base = durationBase(rule.unit);
			const amount =
				base !== null && rule.threshold * base >= 60
					? formatDuration(rule.threshold * base)
					: `${rule.threshold}${rule.unit ? ` ${rule.unit}` : ''}`;
			parts.push(
				rule.operator === '>'
					? m.alerts_rules_op_gt({ amount })
					: rule.operator === '>='
						? m.alerts_rules_op_gte({ amount })
						: rule.operator === '<'
							? m.alerts_rules_op_lt({ amount })
							: m.alerts_rules_op_lte({ amount })
			);
		}
		if (rule.kind === 'log_pattern') {
			parts.push(
				m.logalert_window_scope({
					window: formatDuration(rule.params.log_window_secs ?? 300),
					scope: rule.params.log_per_target === false ? m.logalert_scope_all() : m.logalert_scope_target()
				})
			);
		}
		parts.push(
			rule.for_secs > 0
				? m.alerts_rules_for({ duration: formatDuration(rule.for_secs) })
				: m.alerts_rules_immediately()
		);
		if (rule.repeat_secs) parts.push(m.alerts_rules_repeats({ duration: formatDuration(rule.repeat_secs) }));
		if (rule.escalate_after_secs) {
			parts.push(m.alerts_rules_escalates({ duration: formatDuration(rule.escalate_after_secs) }));
		}
		return parts.join(' · ');
	}

	// --- New rule ------------------------------------------------------------

	let creating = $state(false);
</script>

<div class="mb-4 flex justify-end">
	{#if !auth.can('monitors.manage')}
		<Plate tone="ghost" label={auth.readOnlyLabel} />
	{:else if !creating}
		<Button variant="secondary" size="sm" onclick={() => (creating = true)}>{m.alerts_rules_new()}</Button>
	{/if}
</div>

{#if creating}
	<div class="mb-4">
		<NewRuleForm {oncreate} ondone={() => (creating = false)} />
	</div>
{/if}

{#snippet row(rule: AlertRule, i: number)}
	{@const firing = firingByRule.get(rule.uid) ?? 0}
	{@const ignored = ignoredCount(rule)}
	{@const editing = editingId === rule.id}
	{@const showQuery = shownQueryIds.has(rule.id)}
	<div
		id={`rule-row-${rule.id}`}
		class={`rise-in rounded-[var(--radius-card)] border bg-surface px-4 py-3 shadow-lift max-sm:[contain-intrinsic-size:auto_120px] max-sm:[content-visibility:auto] ${markedId === rule.id ? 'border-signal bg-signal-soft' : editing ? 'border-line-strong' : 'border-line'} ${rule.enabled || editing ? '' : 'opacity-70'}`}
		style={`--rise-delay: ${Math.min(i, 10) * 30}ms`}
	>
		<div class="flex flex-wrap items-start gap-x-4 gap-y-3">
			<div class="min-w-0 flex-[1_1_16rem]">
				<div class="flex flex-wrap items-center gap-2">
					<Plate tone={severityTone(rule.severity)} label={severityWord(rule.severity)} bare />
					<span class="truncate font-semibold text-ink">{rule.name}</span>
					<FileManagedBadge kind="rule" id={rule.uid} />
					{#if rule.builtin}
						<Plate tone="ghost" label={m.alerts_rules_builtin()} bare />
					{/if}
					{#if firing > 0}
						<Plate tone="warning" label={m.alerts_rules_firing_now({ count: firing })} pulse />
					{/if}
					{#if ignored > 0}
						<Plate
							tone="ghost"
							label={ignored > 1
								? m.alerts_rules_ignored_many({ count: ignored })
								: m.alerts_rules_ignored_one()}
							bare
						/>
					{/if}
					{#if savedId === rule.id}
						<Plate tone="signal" label={m.alerts_rules_saved()} bare />
					{/if}
				</div>
				{#if rule.description}
					<p class="mt-1 text-[0.8125rem] text-ink-2">{rule.description}</p>
				{/if}
				<p class="tnum mt-1 text-[0.8125rem] text-ink-2">
					{summary(rule)}
					<span class="text-ink-3" aria-hidden="true">·</span>
					{m.alerts_rules_notifies({ channels: channelsLabel(rule) })}
				</p>
				<p class="mt-0.5 text-[0.8125rem] text-ink-2">
					{m.w1alerts_rules_last_fired({ when: formatRelative(rule.last_fired_at) })}
				</p>
				{#if rule.kind === 'anomaly' && !editing}
					<p class="tnum mt-0.5 text-[0.75rem] text-ink-2">{anomalySummary(rule)}</p>
				{/if}
				<button
					type="button"
					class="mt-1 inline-flex min-h-10 items-center gap-1 text-[0.75rem] font-medium sm:min-h-0 text-ink-2 hover:text-ink hover:underline"
					aria-expanded={showQuery}
					aria-controls={`rule-query-${rule.id}`}
					onclick={() => toggleQuery(rule.id)}
				>
					{#if showQuery}
						<ChevronUp class="size-3.5" aria-hidden="true" />
						{m.alerts_rules_hide_query()}
					{:else}
						<ChevronDown class="size-3.5" aria-hidden="true" />
						{m.alerts_rules_show_query()}
					{/if}
				</button>
				{#if showQuery}
					<p
						id={`rule-query-${rule.id}`}
						class="mt-1 rounded-lg bg-canvas-deep px-2.5 py-1.5 font-mono text-[0.75rem] break-all text-ink-2"
					>
						{rule.query}
					</p>
				{/if}
			</div>

			{#if auth.can('monitors.manage')}
				<div class="flex shrink-0 flex-wrap items-center gap-3">
					{#if !editing}
						<Button variant="ghost" size="sm" onclick={() => (editingId = rule.id)} aria-label={m.alerts_rules_edit_aria({ name: rule.name })}>
							{m.alerts_rules_edit()}
						</Button>
					{/if}
					<Button variant="ghost" size="sm" onclick={() => void clone(rule)} aria-label={m.w1alerts_rules_clone_aria({ name: rule.name })}>
						{m.w1alerts_rules_clone()}
					</Button>
					{#if rule.builtin}
						<Confirm
							size="sm"
							variant="secondary"
							confirmLabel={m.w1alerts_rules_reset_confirm()}
							loading={resettingId === rule.id}
							onconfirm={() => resetToDefaults(rule)}
						>
							{m.w1alerts_rules_reset()}
						</Confirm>
					{/if}
					<Toggle
						id={`rule-toggle-${rule.id}`}
						checked={rule.enabled}
						disabled={busyId === rule.id}
						label={rule.enabled
							? m.alerts_rules_disable_aria({ name: rule.name })
							: m.alerts_rules_enable_aria({ name: rule.name })}
						onchange={(value) => ontoggle(rule, value)}
					/>
					{#if !rule.builtin}
						<Confirm
							size="sm"
							variant="danger"
							confirmLabel={m.alerts_rules_delete_confirm()}
							loading={busyId === rule.id}
							onconfirm={() => ondelete(rule.id)}
						>
							{m.alerts_rules_delete()}
						</Confirm>
					{/if}
				</div>
			{:else}
				<Plate tone={rule.enabled ? 'signal' : 'ghost'} bare label={rule.enabled ? m.alerts_rules_enabled() : m.alerts_rules_disabled()} />
			{/if}
		</div>

		{#if editing}
			<RuleEditor
				{rule}
				channels={pickableChannels}
				{channelsError}
				onsaved={onSaved}
				oncancel={() => ((editingId = null), (backtestId = null))}
				autoBacktest={backtestId === rule.id}
			/>
		{/if}
	</div>
{/snippet}

{#if rows.length === 0}
	<EmptyState
		icon={SlidersHorizontal}
		title={m.alerts_rules_empty_title()}
		description={m.alerts_rules_empty_description()}
		primary={auth.can('monitors.manage') && !creating ? { label: m.alerts_rules_new(), onclick: () => (creating = true) } : undefined}
	/>
{:else}
	<div class="mb-4 flex flex-col gap-3 lg:flex-row lg:items-center">
		<label class="relative min-w-0 flex-1 lg:max-w-sm">
			<span class="sr-only">{m.alerts_rules_search_label()}</span>
			<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
			<input
				type="search"
				data-page-search
				class="input !pl-9"
				placeholder={m.alerts_rules_search_placeholder()}
				bind:value={search}
				autocomplete="off"
			/>
		</label>
		<label class="min-w-0">
			<span class="sr-only">{m.alerts_rules_filter_kind()}</span>
			<select class="input !min-h-10 !w-full !py-1.5 text-sm sm:!min-h-9 sm:!w-auto" bind:value={kindFilter}>
				<option value="">{m.alerts_rules_all_kinds()}</option>
				{#each kindOptions as [value, label] (value)}
					<option {value}>{label}</option>
				{/each}
			</select>
		</label>
	</div>

	<div class="mb-3 flex flex-wrap items-center gap-3">
	<Segmented
		options={quickOptions}
		value={quick}
		onchange={(v) => (quick = v)}
		label={m.alerts_rules_quick_label()}
		size="sm"
		class="max-sm:w-full"
	/>
	<label class="flex items-center gap-2 text-[0.8125rem] text-ink-2">
		<span>{m.w1alerts_rules_sort_label()}</span>
		<select class="input !min-h-10 !w-auto !py-1.5 text-sm sm:!min-h-9" bind:value={sort}>
			{#each sortOptions as option (option.value)}
				<option value={option.value}>{option.label}</option>
			{/each}
		</select>
	</label>
	</div>
	{#if cloneNote}
		<p class="mb-3 text-[0.8125rem] font-medium text-ink" role="status" aria-live="polite">{cloneNote}</p>
	{/if}
	{#if actionError}
		<p class="mb-3 text-[0.8125rem] font-medium text-warning-ink" role="alert">{actionError}</p>
	{/if}
	<p class="tnum mb-3 text-[0.8125rem] text-ink-2" aria-live="polite">
		{m.alerts_rules_count({ shown: shown.length, total: rows.length })}
	</p>

	{#if shown.length === 0}
		<EmptyState
			icon={Search}
			title={m.alerts_rules_nomatch_title()}
			description={m.alerts_rules_nomatch_description()}
			primary={{ label: m.devices_targets_clear_filters(), onclick: () => ((search = ''), (kindFilter = ''), (quick = 'all')) }}
		/>
	{:else}
		<div class="space-y-2">
			{#each groups as [kind, groupRules] (kind)}
				{@const off = groupRules.filter((r) => !r.enabled).length}
				{@const firingHere = groupRules.reduce((sum, r) => sum + (firingByRule.get(r.uid) ?? 0), 0)}
				<details
					class="rounded-[var(--radius-card)] border border-line bg-surface px-4 py-2.5"
					open={isOpen(kind)}
					ontoggle={(e) => setOpen(kind, e.currentTarget.open)}
				>
					<summary class="flex min-h-10 cursor-pointer flex-wrap items-center gap-x-2 gap-y-1 text-sm font-medium text-ink">
						<span class="font-semibold">{groupLabel(kind)}</span>
						<span class="tnum text-ink-2">({groupRules.length})</span>
						{#if firingHere > 0}
							<Plate tone="warning" label={m.alerts_rules_firing_now({ count: firingHere })} pulse />
						{/if}
						{#if off > 0}
							<Plate tone="ghost" bare label={m.alerts_rules_group_off({ count: off })} />
						{/if}
						{#if kind !== '' && !ownedKinds.has(kind)}
							<Plate tone="ghost" bare label={m.alerts_rules_group_no_device()} />
						{/if}
					</summary>
					{#if isOpen(kind)}
						<div class="mt-2.5 space-y-2.5">
							{#each groupRules as rule, i (rule.id)}
								{@render row(rule, i)}
							{/each}
						</div>
					{/if}
				</details>
			{/each}
		</div>
	{/if}
{/if}
