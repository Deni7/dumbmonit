<script lang="ts">
	/**
	 * Coming up: one list, in time order, of what is planned or due. The
	 * maintenance windows of the next 14 days (running ones first), then the
	 * expiries, late backups, disk forecasts (the deadlines and the
	 * dashboards' forecasts) and critical vulnerabilities of the next 90 days,
	 * and a DumbMonit update last (it has no date). It stands for the separate
	 * "Next 14 days", "Maintenance" and "Expiries" cards.
	 *
	 * Each row: the day and how far off, an icon whose shape says the kind,
	 * what and where. Read every five minutes. `fill`: the card takes its
	 * parent's height and scrolls inside.
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
	import { upcomingRows } from '../../upcoming.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { withDiskForecasts } from '../parts/expiries.js';
	import { clock, dayLabel } from '../parts/labels.js';
	import type { Span } from '../parts/sites.js';
	import { upcomingContext, upcomingDue, upcomingTitle } from '../parts/words.js';

	let { span = 4, fill = false, class: cls = '' }: { span?: Span; fill?: boolean; class?: string } = $props();

	const MAINTENANCE_DAYS = 14;
	const DUE_DAYS = 90;
	const REFRESH_MS = 5 * 60_000;
	const MAINTENANCE_HREF = '/alerts?tab=now&maintenance=1';
	const LIMIT = $derived(fill ? 20 : 7);

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
			const to = new Date(at.getTime() + MAINTENANCE_DAYS * 86_400_000).toISOString();
			const [maint, due] = await Promise.all([
				listSilenceOccurrences(from, to, signal)
					.then((list) => {
						occurrences = list;
						return true;
					})
					.catch(() => false),
				listDeadlines({ upcoming: true, maxDays: DUE_DAYS, signal })
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
		/** How far off, or when: "in 31 days", "until 06:00", "available". */
		when: string;
		tone: Tone;
		cve: boolean;
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
				sort: active ? nowMs - 1 : start.getTime(),
				day: active ? m.w9layout_next_now() : dayLabel(start, now),
				when: active ? m.w9layout_next_until({ time: clock(end) }) : clock(start),
				tone: 'maint',
				cve: false,
				title: occurrence.name,
				meta: active ? m.w9layout_next_maint_active() : m.w9layout_next_maint_meta({ duration: formatDuration(seconds) }),
				href: MAINTENANCE_HREF
			});
		}
		const due = withDiskForecasts(
			upcomingRows({ deadlines: deadlines ?? [], findings, update, horizonDays: DUE_DAYS }),
			widgetsStore.data?.top.disk ?? [],
			DUE_DAYS
		);
		for (const row of due) {
			const undated = row.kind === 'update' || row.kind === 'vulnerability';
			// A disk forecast has no date of its own: its days count from now.
			const at = undated ? null : (parseServerDate(row.at) ?? (row.days !== null ? new Date(nowMs + row.days * 86_400_000) : null));
			// Due today: the day says it ("Today"), no second "today" under it.
			const today = row.days !== null && row.days > -1 && row.days < 1;
			out.push({
				key: row.key,
				sort: row.kind === 'update' ? Number.POSITIVE_INFINITY : at ? at.getTime() : nowMs,
				// The update has no day ("available"); a vulnerability is due now, said once.
				day: row.kind === 'update' ? '' : at ? dayLabel(at, now) : m.w9layout_next_now(),
				when: row.kind === 'vulnerability' || (today && at) ? '' : upcomingDue(row),
				tone: row.kind === 'update' ? 'update' : row.tone,
				cve: row.kind === 'vulnerability',
				title: upcomingTitle(row),
				meta: upcomingContext(row),
				href: row.href
			});
		}
		return out.sort((a, b) => a.sort - b.sort || a.key.localeCompare(b.key));
	});
	const shown = $derived(rows.slice(0, LIMIT));

	const ICON = { maint: Wrench, urgent: AlertTriangle, soon: CircleAlert, calm: CalendarClock, update: ArrowUpCircle };
	const CHIP: Record<Tone, string> = {
		maint: 'bg-info-soft text-info-ink',
		urgent: 'bg-warning-soft text-warning-ink',
		soon: 'bg-advisory-soft text-advisory-ink',
		calm: 'bg-surface-2 text-ink-2',
		update: 'bg-info-soft text-info-ink'
	};
	const WHEN: Record<Tone, string> = {
		maint: 'text-ink-2',
		urgent: 'font-semibold text-warning-ink',
		soon: 'font-semibold text-advisory-ink',
		calm: 'text-ink-2',
		update: 'text-ink-2'
	};
	const loading = $derived(occurrences === null && deadlines === null && !failed);
</script>

{#snippet foot()}
	<a href={withBase('/reports')}>{rows.length > LIMIT ? m.w9dash_expiries_more({ count: rows.length }) : m.w9layout_next_deadlines_link()}</a>
	{#if !fill}<a href={withBase(MAINTENANCE_HREF)}>{m.w9layout_next_maintenance_link()}</a>{/if}
{/snippet}

<WidgetCard {span} {fill} flush title={m.overview_upcoming_title()} {foot} class={cls}>
	<DataState {loading} error={failed ? true : null} empty={rows.length === 0} emptyText={m.w10hybrid_coming_empty()} rows={4} class="px-4 pb-3">
		<ul class="m-0 list-none p-0" aria-label={m.overview_upcoming_list_label()}>
			{#each shown as row (row.key)}
				{@const Icon = row.cve ? ShieldAlert : ICON[row.tone]}
				<li class="border-t border-line">
					<a href={withBase(row.href)} class="flex min-h-10 items-center gap-3 px-4 py-2 hover:bg-surface-2">
						<span class={`grid size-6 shrink-0 place-items-center rounded-md ${CHIP[row.tone]}`} aria-hidden="true">
							<Icon class="size-3.5" />
						</span>
						<span class="min-w-0 flex-1">
							<span class="block truncate text-sm font-semibold text-ink">{row.title}</span>
							<span class="block truncate text-[0.8125rem] text-ink-2" title={row.meta}>{row.meta}</span>
						</span>
						<span class="shrink-0 text-right">
							{#if row.day}<span class="block text-[0.8125rem] font-medium whitespace-nowrap text-ink">{row.day}</span>{/if}
							{#if row.when}<span class={`block text-xs whitespace-nowrap ${WHEN[row.tone]}`}>{row.when}</span>{/if}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	</DataState>
</WidgetCard>
