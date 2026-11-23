<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Expiries and forecasts over the next 90 days, soonest first: certificates,
	 * domains, credentials, licences and products reaching their end, backups
	 * running late, critical vulnerabilities, disks forecast to fill (the
	 * deadlines and the dashboards' disk forecasts) and a DumbMonit update.
	 * Each row: the kind, what, one line of context, when, and a bar that
	 * fills as the date comes closer. Same sources as "Coming up", read every
	 * five minutes; the disk forecasts follow the dashboards' store.
	 */
	import { listCves } from '#lib/api/cves.js';
	import { listDeadlines } from '#lib/api/deadlines.js';
	import { getUpdate } from '#lib/api/update.js';
	import type { CveFinding, Deadline, UpdateInfo } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { startPolling } from '#lib/poll.js';
	import { Skeleton } from '#lib/ui/index.js';
	import { upcomingRows, type UpcomingRow } from '../../upcoming.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { withDiskForecasts } from '../parts/expiries.js';
	import { daysLeftFill } from '../parts/format.js';
	import { upcomingContext, upcomingDue, upcomingKind, upcomingTitle } from '../parts/words.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 5, limit = 7 }: { span?: Span; limit?: number } = $props();

	const HORIZON = 90;
	const REFRESH_MS = 5 * 60_000;

	let deadlines = $state<Deadline[] | null>(null);
	let findings = $state<CveFinding[]>([]);
	let update = $state<UpdateInfo | null>(null);
	let failed = $state(false);

	// The deadlines are this card's own data: when they cannot be read, it says so.
	// Vulnerabilities (opt-in, maybe not for this role) and the update check are extras.
	$effect(() =>
		startPolling(async (signal) => {
			const [ok] = await Promise.all([
				listDeadlines({ upcoming: true, maxDays: HORIZON, signal })
					.then((list) => {
						deadlines = list;
						failed = false;
						return true;
					})
					.catch(() => {
						if (!signal.aborted) failed = deadlines === null;
						return false;
					}),
				listCves({ severity: 'critical', state: 'vulnerable', ignored: false }, signal)
					.then((list) => (findings = list.findings))
					.catch(() => {}),
				getUpdate(signal, true)
					.then((info) => (update = info))
					.catch(() => {})
			]);
			return ok;
		}, REFRESH_MS)
	);

	const rows = $derived(
		withDiskForecasts(
			upcomingRows({ deadlines: deadlines ?? [], findings, update, horizonDays: HORIZON }),
			widgetsStore.data?.top.disk ?? [],
			HORIZON
		)
	);
	const shown = $derived(rows.slice(0, limit));

	function ink(row: UpcomingRow): string {
		if (row.kind === 'update') return 'text-info-ink';
		return row.tone === 'urgent' ? 'text-warning-ink' : row.tone === 'soon' ? 'text-advisory-ink' : 'text-ink-2';
	}
	function fill(row: UpcomingRow): string {
		return row.tone === 'urgent' ? 'var(--color-warning)' : row.tone === 'soon' ? 'var(--color-advisory)' : 'var(--color-ink-3)';
	}
</script>

<WidgetCard title={m.w9dash_expiries_title()} sub={m.w9dash_expiries_sub({ days: HORIZON })} {span}>
	{#if deadlines === null && !failed}
		<Skeleton class="h-10 w-full" rows={3} />
	{:else if failed}
		<p class="m-0 py-2 text-sm text-ink-2" role="alert">{m.overview_upcoming_error()}</p>
	{:else if rows.length === 0}
		<p class="m-0 py-2 text-sm font-medium text-ink">{m.overview_upcoming_empty({ days: HORIZON })}</p>
		<p class="m-0 text-xs text-ink-2">{m.overview_upcoming_scope()}</p>
	{:else}
		<ul class="m-0 list-none p-0" aria-label={m.overview_upcoming_list_label()}>
			{#each shown as row, i (row.key)}
				<li class={i > 0 ? 'border-t border-line' : ''}>
					<a href={withBase(row.href)} class="-mx-2 flex min-h-10 items-center gap-2.5 rounded-lg px-2 py-2 hover:bg-surface-2">
						<span class="min-w-0 flex-1">
							<span class="block truncate text-xs text-ink-2">{upcomingKind(row)}</span>
							<span class="block truncate text-sm font-semibold text-ink">{upcomingTitle(row)}</span>
							<span class="block truncate text-[0.8125rem] text-ink-2" title={upcomingContext(row)}>{upcomingContext(row)}</span>
						</span>
						<span class="shrink-0 text-right">
							<span class={`block font-mono text-sm font-semibold whitespace-nowrap tabular-nums ${ink(row)}`}>{upcomingDue(row)}</span>
							{#if row.days !== null && row.kind !== 'vulnerability'}
								<span class="mt-1.5 ml-auto block h-1.5 w-16 overflow-hidden rounded-full bg-ghost" title={m.w9dash_expiries_bar_hint({ days: HORIZON })}>
									<span class="block h-full rounded-full" style={`width:${daysLeftFill(row.days, HORIZON)}%;background:${fill(row)}`}></span>
								</span>
							{/if}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet foot()}
		<a href={withBase('/reports')}>{rows.length > limit ? m.w9dash_expiries_more({ count: rows.length }) : m.w9dash_expiries_all()}</a>
		<a href={withBase('/security')}>{m.w9dash_expiries_vulns()}</a>
	{/snippet}
</WidgetCard>
