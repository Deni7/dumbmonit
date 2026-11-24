<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * One row of "Act now": severity, monitor (its kind
	 * and site), the problem, how long it has lasted, who took it (or that
	 * nobody has, and whether anyone was told), and the quick actions. Wide
	 * panels lay it out as a table row; narrow ones (a container query) stack
	 * it. Sizes from `scale.ts`; the taken / not taken line has the alerts
	 * page's icons.
	 */
	import { AlertTriangle, BellOff, CheckCheck } from 'lucide-svelte';
	import AlertActions from '#lib/components/alerts/AlertActions.svelte';
	import { alertDetail, severityTone, severityWord } from '#lib/components/alerts/helpers.js';
	import type { Silence } from '#lib/api/index.js';
	import { formatRelative, parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, Plate } from '#lib/ui/index.js';
	import MuteControl from '../../MuteControl.svelte';
	import { actorName, initials, type ProblemRow } from './problems.js';
	import { INLINE_ICON, ROW, TEXT_BODY, TEXT_MAIN, TEXT_SUB } from './scale.js';
	import type { MonitorPlace } from './site-index.js';
	import { clockText, durationText } from './words.js';

	interface Props {
		problem: ProblemRow;
		place: MonitorPlace | undefined;
		/** The window muting this monitor now (device rows). */
		muted: Silence | null;
		/** Ticks every few seconds so the age keeps moving. */
		now: number;
		busy: boolean;
		failure: string | null;
		onacknowledge: () => void;
		onmuted: (silence: Silence) => void;
		onunmuted: (silence: Silence) => void;
		onacknowledged: (silence: Silence) => void;
		onfail: (message: string | null) => void;
	}

	let { problem, place, muted, now, busy, failure, onacknowledge, onmuted, onunmuted, onacknowledged, onfail }: Props = $props();

	const row = $derived(problem.row);
	const tone = $derived(row.kind === 'alert' ? severityTone(row.alert.severity) : row.tone);
	const plate = $derived(row.kind === 'alert' ? severityWord(row.alert.severity) : row.plate);
	const target = $derived(row.target);
	const name = $derived(target?.name ?? (row.kind === 'alert' ? row.alert.rule_name : ''));
	const href = $derived(target ? `/targets/${target.id}` : '/alerts');
	const what = $derived.by(() => {
		if (row.kind === 'device') return row.detail || m.w9dash_problems_no_answer();
		return row.alert.rule_name || row.rule?.description || '';
	});
	const detail = $derived.by(() => {
		if (row.kind === 'device') return '';
		return alertDetail(row.alert, row.rule) || (row.alert.rule_name ? (row.rule?.description ?? '') : '');
	});
	const since = $derived(parseServerDate(row.since));
	const age = $derived(since ? durationText(Math.max(0, (now - since.getTime()) / 1000)) : '');
	const who = $derived(actorName(problem.acked?.by));
	const notifiedAt = $derived(row.kind === 'alert' ? row.alert.last_notified_at : null);
	const meta = `block truncate ${TEXT_SUB}`;
	const takenLine = 'inline-flex items-center gap-1.5 text-[0.8125rem] font-semibold';
</script>

<li class={`grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3.5 gap-y-1 ${ROW} @3xl:grid-cols-[6.5rem_minmax(0,1.15fr)_minmax(0,1.7fr)_4.75rem_minmax(0,1.15fr)_auto] ${problem.acked || problem.muted ? 'bg-surface-2/40' : ''}`}>
	<div class="order-1 min-w-0 @3xl:order-none">
		<Plate {tone} label={plate} class="max-w-full min-w-0 truncate" />
	</div>
	<div class="order-3 col-span-2 min-w-0 @3xl:order-none @3xl:col-span-1">
		<a href={withBase(href)} class={`block truncate hover:underline ${TEXT_MAIN}`}>{name}</a>
		{#if place || problem.held > 0}
			<span class={meta}>
				{#if place}{m.w9dash_problems_place({ kind: place.kind, site: place.siteName })}{/if}
				{#if problem.held > 0}<span class="text-ink-3"> {m.overview_needs_held({ count: problem.held })}</span>{/if}
			</span>
		{/if}
	</div>
	<div class="order-4 col-span-2 min-w-0 @3xl:order-none @3xl:col-span-1">
		<span class={`block truncate ${TEXT_BODY}`} title={what}>{what}</span>
		{#if detail}<span class={meta} title={detail}>{detail}</span>{/if}
	</div>
	<div class="order-2 min-w-0 text-right @3xl:order-none @3xl:text-left">
		{#if since}
			<span class="block text-sm font-medium whitespace-nowrap text-ink tabular-nums">{age}</span>
			<span class={meta}>{m.w9dash_problems_since({ time: clockText(since) })}</span>
		{/if}
	</div>
	<div class="order-5 col-span-2 min-w-0 @3xl:order-none @3xl:col-span-1">
		{#if problem.acked}
			<span class={`${takenLine} text-ink-2`}>
				<CheckCheck class={INLINE_ICON} aria-hidden="true" />{m.overview_sky_plate_acked()}
			</span>
			<span class={`flex min-w-0 items-center gap-1 ${TEXT_SUB}`}>
				{#if who}
					<span aria-hidden="true" class="inline-grid size-5 shrink-0 place-items-center rounded-full bg-surface-2 text-[0.625rem] font-bold text-ink">{initials(who)}</span>
					<span class="truncate">{who}</span>
				{:else}
					<span class="truncate">{m.w9dash_problems_ack_recovery()}</span>
				{/if}
			</span>
		{:else if problem.muted}
			<span class={`${takenLine} text-ink-2`}>
				<BellOff class={INLINE_ICON} aria-hidden="true" />{m.overview_sky_plate_snoozed()}
			</span>
		{:else}
			<span class={`${takenLine} text-warning-ink`}>
				<AlertTriangle class={INLINE_ICON} aria-hidden="true" />{m.w9dash_problems_not_acked()}
			</span>
			<span class={meta}>
				{notifiedAt ? m.w9dash_problems_notified({ ago: formatRelative(notifiedAt) }) : m.w9dash_problems_not_notified()}
			</span>
		{/if}
	</div>
	<div class="order-6 col-span-2 flex flex-wrap items-center justify-start gap-1.5 @3xl:order-none @3xl:col-span-1 @3xl:justify-end">
		{#if row.kind === 'alert'}
			<AlertActions alert={row.alert} target={row.target} labelled onchanged={() => void alertsStore.refresh()} />
		{:else if auth.canOperate}
			{#if !problem.acked}
				<Button size="sm" variant="secondary" loading={busy} onclick={onacknowledge}>{m.overview_ack_item()}</Button>
			{/if}
			<MuteControl target={row.target} {muted} {onmuted} {onunmuted} {onacknowledged} {onfail} />
		{/if}
	</div>
	{#if failure}
		<p class="order-7 col-span-2 m-0 text-xs text-warning-ink @3xl:col-span-6" role="alert">{failure}</p>
	{/if}
</li>
