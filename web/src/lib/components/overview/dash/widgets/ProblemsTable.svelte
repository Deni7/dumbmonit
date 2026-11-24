<script lang="ts">
	/**
	 * "Act now", the overview's largest panel: what is broken now, worst
	 * first, one row each (see `parts/problems.ts`), with who took it and the
	 * quick actions: acknowledge, snooze or mute. A taken problem stays
	 * listed, below the open ones, so the table says who is on what. The count
	 * is the "Open problems" figure above it; the title leads to the Alerts
	 * page. With nothing broken: "Nothing needs you", and the last alerts
	 * that resolved.
	 *
	 * Monitors and alerts come from the app-wide store; sites and kinds from
	 * the widgets store; the history from the fleet store.
	 */
	import { untrack } from 'svelte';
	import { CircleCheck } from 'lucide-svelte';
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
	import ProblemLine from '../parts/ProblemLine.svelte';
	import { uniqueKeys } from '../parts/keys.js';
	import { problemRows, recentResolved, type ProblemRow } from '../parts/problems.js';
	import { ROW, ROW_ICON, STATE_PAD, TEXT_MAIN, TEXT_SIDE, TEXT_SUB } from '../parts/scale.js';
	import { siteIndex } from '../parts/site-index.js';
	import { clockText, durationText } from '../parts/words.js';

	let { class: cls = '' }: { class?: string } = $props();

	/** Rows drawn (the panel scrolls); the rest is on the Alerts page. */
	const LIMIT = 30;

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

	// A problem's key names its monitor or its alert; made unique all the same,
	// so a row that comes twice can never stop the page (`parts/keys.ts`).
	const keys = $derived(uniqueKeys(shown, (problem) => problem.row.key));

	// --- Calm state -----------------------------------------------------------
	const targetNames = $derived(new Map(alertsStore.targets.map((t) => [t.id, t.name])));
	const ruleNames = $derived(new Map(rules.map((r) => [r.uid, r.name])));
	const resolved = $derived(recentResolved(fleetStore.history, 3));
	const resolvedKeys = $derived(uniqueKeys(resolved, (entry) => `${entry.fingerprint}@${entry.at}`));

	const reading = $derived(alertsStore.loading && alertsStore.targets.length === 0);
	const broken = $derived(!alertsStore.available && alertsStore.targets.length === 0 && alertsStore.lastError);
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

<WidgetCard title={m.w9layout_hybrid_act_now()} href="/alerts" class={cls}>
	{#if reading}
		<div class={`flex flex-col gap-2.5 border-t border-line ${STATE_PAD}`} aria-hidden="true">
			<Skeleton class="h-9 w-full" rows={3} />
		</div>
	{:else if broken}
		<div class={`border-t border-line ${STATE_PAD}`}>
			<ErrorNotice error={alertsStore.lastError} compact onretry={() => void alertsStore.refresh()} />
		</div>
	{:else if rows.length === 0}
		<div class="state-in">
			<div class={`flex items-center gap-3 bg-signal-soft ${ROW}`}>
				<CircleCheck class={`${ROW_ICON} text-signal-ink`} aria-hidden="true" />
				<div class="min-w-0">
					<p class={`m-0 ${TEXT_MAIN}`}>{m.w9dash_problems_calm_title()}</p>
					<p class={`m-0 ${TEXT_SUB}`}>
						{resolved[0] ? m.w9dash_problems_calm_since({ time: clockText(resolved[0].at) }) : m.w9dash_problems_calm_text()}
					</p>
				</div>
			</div>
			{#if resolved.length > 0}
				<h3 class="m-0 border-t border-line px-4 pt-3 pb-1.5 text-xs font-medium text-ink-3">{m.w9dash_problems_resolved()}</h3>
				<ul class="m-0 list-none p-0">
					{#each resolved as entry, i (resolvedKeys[i])}
						{@const name = (entry.target_id !== null ? targetNames.get(entry.target_id) : undefined) ?? ruleNames.get(entry.rule_uid) ?? ''}
						<li class={`flex min-w-0 items-center gap-3 ${ROW}`}>
							<Plate tone="signal" label={m.w9dash_problems_resolved_plate()} class="shrink-0" />
							<div class="min-w-0 flex-1">
								<p class="m-0 truncate text-sm">
									<span class={TEXT_MAIN}>{name}</span>
									{#if entry.target_id !== null && ruleNames.get(entry.rule_uid)}<span class="text-ink-2"> {ruleNames.get(entry.rule_uid)}</span>{/if}
								</p>
								{#if entry.lasted !== null}
									<span class={`block truncate ${TEXT_SUB}`}>{m.w9dash_problems_lasted({ span: durationText(entry.lasted) })}</span>
								{/if}
							</div>
							<span class={`shrink-0 text-right text-ink-2 ${TEXT_SIDE}`}>{clockText(entry.at)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{:else}
		<div class="@container">
			<div class="sticky top-0 z-[1] hidden gap-3.5 bg-surface px-4 pb-1.5 text-xs font-medium text-ink-3 @3xl:grid @3xl:grid-cols-[6.5rem_minmax(0,1.15fr)_minmax(0,1.7fr)_4.75rem_minmax(0,1.15fr)_auto]" aria-hidden="true">
				<span>{m.w9dash_problems_col_severity()}</span>
				<span>{m.w9dash_problems_col_monitor()}</span>
				<span>{m.w9dash_problems_col_problem()}</span>
				<span>{m.w9dash_problems_col_age()}</span>
				<span>{m.w9dash_problems_col_acked()}</span>
				<span></span>
			</div>
			<ul class="m-0 list-none p-0">
				{#each shown as problem, i (keys[i])}{@render line(problem)}{/each}
			</ul>
		</div>
	{/if}
</WidgetCard>
