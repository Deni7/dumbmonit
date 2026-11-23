<script lang="ts">
	/**
	 * Next 14 days: what is planned or due soon, in time order. Maintenance
	 * windows (from the calendar of silences), expiries, disk forecasts and
	 * late backups (deadlines), critical vulnerabilities (due now) and a
	 * DumbMonit update (last, it has no date). Each row: the day and time, an
	 * icon whose shape says the kind, what and where. Refreshed every 5 minutes.
	 */
	import { AlertTriangle, ArrowUpCircle, CalendarClock, CircleAlert, ShieldAlert, Wrench } from 'lucide-svelte';
	import { listCves } from '#lib/api/cves.js';
	import { listDeadlines } from '#lib/api/deadlines.js';
	import { getUpdate } from '#lib/api/update.js';
	import { listSilenceOccurrences, type CveFinding, type Deadline, type SilenceOccurrence, type UpdateInfo } from '#lib/api/index.js';
	import { formatDuration, parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { startPolling } from '#lib/poll.js';
	import WidgetCard from '../WidgetCard.svelte';
	import DataState from '../parts/DataState.svelte';
	import { clock, dayLabel } from '../parts/labels.js';
	import { upcomingContext, upcomingDue, upcomingTitle } from '../parts/upcoming-text.js';
	import type { Span } from '../parts/sites.js';
	import { upcomingRows } from '../../upcoming.js';

	let { span = 4 }: { span?: Span } = $props();

	const HORIZON_DAYS = 14;
	const LIMIT = 7;
	const REFRESH_MS = 5 * 60_000;
	const MAINTENANCE_HREF = '/alerts?tab=now&maintenance=1';

	let occurrences = $state<SilenceOccurrence[] | null>(null);
	let deadlines = $state<Deadline[] | null>(null);
	let findings = $state<CveFinding[]>([]);
	let update = $state<UpdateInfo | null>(null);
	let failed = $state(false);
	let now = $state(new Date());

	// Maintenance and deadlines are the card's own data: only both failing shows the error
	// line. Vulnerabilities and the update are extras, silent when absent (off, or not for this role).
	$effect(() =>
		startPolling(async (signal) => {
			// A local copy: the effect must not read the state it writes.
			const at = new Date();
			now = at;
			const from = at.toISOString();
			const to = new Date(at.getTime() + HORIZON_DAYS * 86_400_000).toISOString();
			const [maint, due] = await Promise.all([
				listSilenceOccurrences(from, to, signal)
					.then((list) => {
						occurrences = list;
						return true;
					})
					.catch(() => false),
				listDeadlines({ upcoming: true, maxDays: HORIZON_DAYS, signal })
					.then((list) => {
						deadlines = list;
						return true;
					})
					.catch(() => false),
				listCves({ severity: 'critical', state: 'vulnerable', ignored: false }, signal)
					.then((list) => (findings = list.findings))
					.catch(() => {}),
				getUpdate(signal, true)
					.then((info) => (update = info))
					.catch(() => {})
			]);
			if (signal.aborted) return true;
			failed = !maint && !due && occurrences === null && deadlines === null;
			return maint || due;
		}, REFRESH_MS)
	);

	type Tone = 'maint' | 'urgent' | 'soon' | 'calm' | 'update';

	interface Row {
		key: string;
		/** Sort key in ms; due-now items at `now`, the update last. */
		sort: number;
		day: string;
		time: string;
		tone: Tone;
		title: string;
		meta: string;
		href: string;
	}

	const rows = $derived.by<Row[]>(() => {
		const out: Row[] = [];
		const nowMs = now.getTime();
		for (const occurrence of occurrences ?? []) {
			if (!occurrence.enabled || occurrence.state === 'past') continue;
			const start = parseServerDate(occurrence.starts_at);
			const end = parseServerDate(occurrence.ends_at);
			if (!start || !end) continue;
			const active = occurrence.state === 'active';
			const seconds = Math.max(0, (end.getTime() - start.getTime()) / 1000);
			out.push({
				key: `maint:${occurrence.silence_id}:${occurrence.starts_at}`,
				sort: active ? nowMs : start.getTime(),
				day: active ? m.w9layout_next_now() : dayLabel(start, now),
				time: active ? m.w9layout_next_until({ time: clock(end) }) : clock(start),
				tone: 'maint',
				title: occurrence.name,
				meta: active ? m.w9layout_next_maint_active() : m.w9layout_next_maint_meta({ duration: formatDuration(seconds) }),
				href: MAINTENANCE_HREF
			});
		}
		for (const row of upcomingRows({ deadlines: deadlines ?? [], findings, update, horizonDays: HORIZON_DAYS })) {
			const at = row.kind === 'vulnerability' || row.kind === 'update' ? null : parseServerDate(row.at);
			out.push({
				key: row.key,
				sort: row.kind === 'update' ? Number.POSITIVE_INFINITY : at ? at.getTime() : nowMs,
				day: at && (row.days ?? 0) > 0 ? dayLabel(at, now) : m.w9layout_next_now(),
				time: row.kind === 'update' || row.kind === 'vulnerability' ? '' : upcomingDue(row),
				tone: row.kind === 'update' ? 'update' : row.tone,
				title: upcomingTitle(row),
				meta: upcomingContext(row),
				href: row.href
			});
		}
		return out.sort((a, b) => a.sort - b.sort || a.key.localeCompare(b.key)).slice(0, LIMIT);
	});

	const ICON = { maint: Wrench, urgent: AlertTriangle, soon: CircleAlert, calm: CalendarClock, update: ArrowUpCircle };
	const CHIP: Record<Tone, string> = {
		maint: 'bg-info-soft text-info-ink',
		urgent: 'bg-warning-soft text-warning-ink',
		soon: 'bg-advisory-soft text-advisory-ink',
		calm: 'bg-surface-2 text-ink-2',
		update: 'bg-info-soft text-info-ink'
	};
	const loading = $derived(occurrences === null && deadlines === null && !failed);
</script>

{#snippet foot()}
	<a href={withBase(MAINTENANCE_HREF)}>{m.w9layout_next_maintenance_link()}</a>
	<a href={withBase('/reports')}>{m.w9layout_next_deadlines_link()}</a>
{/snippet}

<WidgetCard {span} title={m.w9layout_next_title({ count: HORIZON_DAYS })} {foot}>
	<DataState {loading} error={failed ? true : null} empty={rows.length === 0} emptyText={m.w9layout_next_empty({ count: HORIZON_DAYS })} rows={5}>
		<ul class="m-0 list-none p-0">
			{#each rows as row (row.key)}
				{@const Icon = row.key.startsWith('cve:') ? ShieldAlert : ICON[row.tone]}
				<li class="border-t border-line first:border-t-0">
					<a href={withBase(row.href)} class="flex min-h-10 items-center gap-2.5 py-2 hover:bg-surface-2">
						<span class="w-[4.5rem] shrink-0">
							<b class="block truncate text-[0.8125rem] font-semibold text-ink">{row.day}</b>
							{#if row.time}<span class="block truncate text-xs text-ink-2">{row.time}</span>{/if}
						</span>
						<span class={`grid size-5 shrink-0 place-items-center rounded-md ${CHIP[row.tone]}`} aria-hidden="true">
							<Icon class="size-3" />
						</span>
						<span class="min-w-0 flex-1">
							<b class="block truncate font-semibold text-ink">{row.title}</b>
							<span class="block truncate text-xs text-ink-2" title={row.meta}>{row.meta}</span>
						</span>
					</a>
				</li>
			{/each}
		</ul>
	</DataState>
</WidgetCard>
