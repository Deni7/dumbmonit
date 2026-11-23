<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The problems table of the dashboards: what is broken now, worst first,
	 * one row each (see `parts/problems.ts`), with who took it and the quick
	 * actions: acknowledge, snooze or mute. Unlike "Needs attention", a taken
	 * problem stays listed, below the open ones, so the table says who is on
	 * what. `bySite` groups the rows under their site. With nothing broken:
	 * "Nothing needs you", and the last alerts that resolved.
	 *
	 * Monitors and alerts come from the app-wide store; sites and kinds from
	 * the dashboards' store; the history from the fleet store.
	 */
	import { untrack } from 'svelte';
	import { createSilence, listAlertRules, listSilences, toApiError, type AlertRule, type Silence, type Target } from '#lib/api/index.js';
	import { recoverySilencePayload } from '#lib/components/alerts/helpers.js';
	import { fleetStore } from '#lib/components/enterprise/fleet.svelte.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';
	import { acknowledgedBy, mutedBy } from '../../mute.js';
	import { readSky } from '../../sky.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import Glyph from '../parts/Glyph.svelte';
	import ProblemLine from '../parts/ProblemLine.svelte';
	import { problemRows, recentResolved, type ProblemRow } from '../parts/problems.js';
	import { siteIndex } from '../parts/site-index.js';
	import { clockText, durationText } from '../parts/words.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	interface Props {
		span?: Span;
		title?: string;
		bySite?: boolean;
	}

	let { span = 8, title, bySite = false }: Props = $props();

	/** Rows drawn; the rest is one link away. */
	const LIMIT = 8;

	$effect(() => untrack(() => fleetStore.start()));

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 15_000);
		return () => clearInterval(timer);
	});

	let rules = $state<AlertRule[]>([]);
	$effect(() => {
		const controller = new AbortController();
		listAlertRules(controller.signal)
			.then((list) => (rules = list))
			.catch(() => {});
		return () => controller.abort();
	});

	// The windows come from the server (read again with each poll of the
	// monitors: it lifts an acknowledgement by itself when a monitor answers);
	// the ones this table just created are remembered too, in case the server's
	// clock is a few seconds behind.
	let silences = $state<Silence[]>([]);
	let justCreated = $state<number[]>([]);
	let failure = $state<{ id: number; message: string } | null>(null);
	let acking = $state<number | null>(null);
	$effect(() => {
		void alertsStore.targets;
		const controller = new AbortController();
		listSilences(controller.signal)
			.then((list) => (silences = list))
			.catch(() => {});
		return () => controller.abort();
	});

	function keep(silence: Silence) {
		silences = [...silences.filter((s) => s.id !== silence.id), silence];
		justCreated = [...justCreated, silence.id];
		void alertsStore.refresh();
	}
	function drop(silence: Silence) {
		silences = silences.filter((s) => s.id !== silence.id);
		justCreated = justCreated.filter((id) => id !== silence.id);
		void alertsStore.refresh();
	}
	async function acknowledge(target: Target) {
		if (acking !== null) return;
		acking = target.id;
		failure = null;
		try {
			keep(await createSilence(recoverySilencePayload(target)));
		} catch (cause) {
			failure = { id: target.id, message: toApiError(cause).message || m.overview_ack_err() };
		} finally {
			acking = null;
		}
	}

	const sky = $derived(readSky({ targets: alertsStore.targets, probes: alertsStore.probes, alerts: alertsStore.alerts, rules }));
	const ackedTargets = $derived(
		new Set(alertsStore.targets.filter((t) => acknowledgedBy(silences, t.id, justCreated)).map((t) => t.id))
	);
	const rows = $derived(problemRows(sky.needsYou, ackedTargets));
	const shown = $derived(rows.slice(0, LIMIT));
	const places = $derived(siteIndex(widgetsStore.data?.sites ?? []));

	function targetOf(problem: ProblemRow): number | null {
		return problem.row.target?.id ?? null;
	}

	/** Rows under their site, in the order of the sites; rows tied to no known site last. */
	const groups = $derived.by(() => {
		const sites = widgetsStore.data?.sites ?? [];
		const out = sites.map((site) => ({ key: site.key, name: site.name, rows: [] as ProblemRow[] }));
		const byKey = new Map(out.map((g) => [g.key, g]));
		const other = { key: '_other', name: m.w9dash_problems_no_site(), rows: [] as ProblemRow[] };
		for (const problem of shown) {
			const id = targetOf(problem);
			const place = id !== null ? places.get(id) : undefined;
			const group = place ? byKey.get(place.siteKey) : undefined;
			if (group) group.rows.push(problem);
			else other.rows.push(problem);
		}
		if (other.rows.length > 0) out.push(other);
		return out;
	});

	// --- Calm state -----------------------------------------------------------
	const targetNames = $derived(new Map(alertsStore.targets.map((t) => [t.id, t.name])));
	const ruleNames = $derived(new Map(rules.map((r) => [r.uid, r.name])));
	const resolved = $derived(recentResolved(fleetStore.history, 3));

	const reading = $derived(alertsStore.loading && alertsStore.targets.length === 0);
	const broken = $derived(!alertsStore.available && alertsStore.targets.length === 0 && alertsStore.lastError);
	const sub = $derived(rows.length > 0 ? m.w9dash_problems_sub({ count: rows.length }) : undefined);
</script>

{#snippet line(problem: ProblemRow)}
	{@const id = targetOf(problem)}
	{@const device = problem.row.kind === 'device' ? problem.row.target : null}
	<ProblemLine
		{problem}
		place={id !== null ? places.get(id) : undefined}
		muted={device ? mutedBy(silences, device.id, justCreated) : null}
		{now}
		busy={device !== null && acking === device.id}
		failure={failure && id !== null && failure.id === id ? failure.message : null}
		onacknowledge={() => {
			if (device) void acknowledge(device);
		}}
		onmuted={keep}
		onunmuted={drop}
		onacknowledged={keep}
		onfail={(message) => (failure = message && id !== null ? { id, message } : null)}
	/>
{/snippet}

<WidgetCard title={title ?? m.w9dash_problems_title()} {sub} {span} flush>
	{#if reading}
		<div class="flex flex-col gap-2.5 px-4 pb-4" aria-hidden="true">
			<Skeleton class="h-9 w-full" rows={3} />
		</div>
	{:else if broken}
		<div class="px-4 pb-4">
			<ErrorNotice error={alertsStore.lastError} compact onretry={() => void alertsStore.refresh()} />
		</div>
	{:else if rows.length === 0}
		<div class="state-in px-4 pb-4">
			<div class="flex items-center gap-3 rounded-xl bg-signal-soft p-3.5 text-signal-ink">
				<Glyph name="ack" class="size-8" />
				<div class="min-w-0">
					<p class="m-0 text-base font-semibold text-ink">{m.w9dash_problems_calm_title()}</p>
					<p class="m-0 text-[0.8125rem] text-ink-2">
						{resolved[0] ? m.w9dash_problems_calm_since({ time: clockText(resolved[0].at) }) : m.w9dash_problems_calm_text()}
					</p>
				</div>
			</div>
			{#if resolved.length > 0}
				<h3 class="m-0 mt-3 mb-1 text-[0.8125rem] font-semibold text-ink">{m.w9dash_problems_resolved()}</h3>
				<ul class="m-0 list-none p-0">
					{#each resolved as entry, i (entry.fingerprint + entry.at)}
						{@const name = (entry.target_id !== null ? targetNames.get(entry.target_id) : undefined) ?? ruleNames.get(entry.rule_uid) ?? ''}
						<li class={`flex min-w-0 items-center gap-2.5 py-2 ${i > 0 ? 'border-t border-line' : ''}`}>
							<Plate tone="signal" label={m.w9dash_problems_resolved_plate()} />
							<div class="min-w-0 flex-1">
								<p class="m-0 truncate text-sm">
									<span class="font-semibold text-ink">{name}</span>
									{#if entry.target_id !== null && ruleNames.get(entry.rule_uid)}<span class="text-ink-3"> {ruleNames.get(entry.rule_uid)}</span>{/if}
								</p>
								{#if entry.lasted !== null}
									<span class="block truncate text-[0.8125rem] text-ink-2">{m.w9dash_problems_lasted({ span: durationText(entry.lasted) })}</span>
								{/if}
							</div>
							<span class="shrink-0 text-right text-[0.8125rem] whitespace-nowrap text-ink-3">{clockText(entry.at)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{:else}
		<div class="@container">
			<div class="hidden gap-3.5 px-4 pb-1.5 text-xs font-medium text-ink-3 @3xl:grid @3xl:grid-cols-[6.5rem_minmax(0,1.15fr)_minmax(0,1.7fr)_4.75rem_minmax(0,1.15fr)_auto]" aria-hidden="true">
				<span>{m.w9dash_problems_col_severity()}</span>
				<span>{m.w9dash_problems_col_monitor()}</span>
				<span>{m.w9dash_problems_col_problem()}</span>
				<span>{m.w9dash_problems_col_age()}</span>
				<span>{m.w9dash_problems_col_acked()}</span>
				<span></span>
			</div>
			{#if bySite && (widgetsStore.data?.sites.length ?? 0) > 0}
				{#each groups as group (group.key)}
					<div class="flex items-center gap-2 border-t border-line bg-surface-2 px-4 py-2 text-[0.8125rem] font-semibold text-ink-2">
						{group.name}
						{#if group.rows.length === 0}<span class="font-normal text-ink-3">{m.w9dash_problems_site_clear()}</span>{/if}
					</div>
					{#if group.rows.length > 0}
						<ul class="m-0 list-none p-0">
							{#each group.rows as problem (problem.row.key)}{@render line(problem)}{/each}
						</ul>
					{/if}
				{/each}
			{:else}
				<ul class="m-0 list-none p-0">
					{#each shown as problem (problem.row.key)}{@render line(problem)}{/each}
				</ul>
			{/if}
		</div>
	{/if}
	{#snippet foot()}
		<a href={withBase('/alerts')}>{rows.length > LIMIT ? m.overview_needs_more({ count: rows.length }) : m.w9dash_common_all_alerts()}</a>
		<a href={withBase('/alerts?tab=now&maintenance=1')}>{m.w9dash_problems_maintenance_link()}</a>
	{/snippet}
</WidgetCard>
