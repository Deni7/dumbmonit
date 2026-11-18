<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * "Coming up": what is about to become a problem, most urgent first, so it
	 * is dealt with before it turns into an alert. Expiring certificates and
	 * domains, disks the forecast says will fill, backups that stopped running,
	 * critical vulnerabilities and a DumbMonit update (see `upcoming.ts`).
	 * Each row: the kind, what and where, one line of context, when. The date
	 * is a word ("in 6 days", "3 days ago"); its colour only backs it up.
	 */
	import { ArrowRight } from 'lucide-svelte';
	import { listCves } from '#lib/api/cves.js';
	import { listDeadlines } from '#lib/api/deadlines.js';
	import { getUpdate } from '#lib/api/update.js';
	import type { CveFinding, Deadline, UpdateInfo } from '#lib/api/index.js';
	import { parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Skeleton } from '#lib/ui/index.js';
	import { HORIZON_DAYS, ROW_LIMIT, upcomingRows, type UpcomingRow, type UpcomingTone } from './upcoming.js';

	const REFRESH_MS = 5 * 60_000;

	let deadlines = $state<Deadline[] | null>(null);
	let findings = $state<CveFinding[]>([]);
	let update = $state<UpdateInfo | null>(null);
	let failed = $state(false);

	// The deadlines are the block's own data: when they cannot be read, say so rather than
	// "nothing due". The vulnerabilities (an opt-in feature, possibly off or not for this
	// role) and the update check are extras: silent when absent.
	$effect(() => {
		const controller = new AbortController();
		const load = () => {
			listDeadlines({ upcoming: true, maxDays: HORIZON_DAYS, signal: controller.signal })
				.then((list) => {
					deadlines = list;
					failed = false;
				})
				.catch(() => {
					if (!controller.signal.aborted) failed = deadlines === null;
				});
			listCves({ severity: 'critical', state: 'vulnerable', ignored: false }, controller.signal)
				.then((list) => (findings = list.findings))
				.catch(() => {});
			getUpdate(controller.signal, true)
				.then((info) => (update = info))
				.catch(() => {});
		};
		load();
		const timer = setInterval(load, REFRESH_MS);
		return () => {
			controller.abort();
			clearInterval(timer);
		};
	});

	const rows = $derived(upcomingRows({ deadlines: deadlines ?? [], findings, update }));
	const shown = $derived(rows.slice(0, ROW_LIMIT));

	function kindLabel(row: UpcomingRow): string {
		switch (row.kind) {
			case 'certificate':
				return m.deadlines_kind_certificate();
			case 'domain':
				return m.deadlines_kind_domain();
			case 'credential':
				return m.deadlines_kind_credential();
			case 'product':
				return m.deadlines_kind_product();
			case 'key':
				return m.deadlines_kind_key();
			case 'license':
				return m.deadlines_kind_license();
			case 'disk':
				return m.overview_upcoming_kind_disk();
			case 'backup':
				return m.overview_upcoming_kind_backup();
			case 'vulnerability':
				return m.overview_upcoming_kind_vulnerability();
			case 'update':
				return m.overview_upcoming_kind_update();
			default:
				return row.kind;
		}
	}

	/** The date a row falls due, as the reader's locale writes it. */
	function dueDate(row: UpcomingRow): string {
		const at = parseServerDate(row.at);
		return at ? at.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }) : '';
	}

	function title(row: UpcomingRow): string {
		const late = row.days !== null && row.days <= 0;
		switch (row.kind) {
			case 'disk':
				return late ? m.overview_upcoming_t_disk_full({ what: row.what }) : m.overview_upcoming_t_disk({ what: row.what });
			case 'backup':
				return late ? m.overview_upcoming_t_backup_late({ what: row.what }) : m.overview_upcoming_t_backup({ what: row.what });
			case 'vulnerability':
				return m.overview_upcoming_t_vuln({ count: row.count ?? 0 });
			case 'update':
				return m.overview_upcoming_t_update({ version: row.latest ?? '' });
			default:
				return late ? m.overview_upcoming_t_expired({ what: row.what }) : m.overview_upcoming_t_expires({ what: row.what });
		}
	}

	function context(row: UpcomingRow): string {
		const monitor = row.monitor ?? '';
		const late = row.days !== null && row.days <= 0;
		switch (row.kind) {
			case 'disk':
				return m.overview_upcoming_c_disk({ percent: Math.round(row.value ?? 0), monitor });
			case 'backup':
				return m.overview_upcoming_c_backup({ count: Math.max(1, Math.round(row.value ?? 0)), monitor });
			case 'vulnerability':
				return row.exploited ? m.overview_upcoming_c_vuln_exploited({ monitor }) : m.overview_upcoming_c_vuln({ monitor });
			case 'update':
				return m.overview_upcoming_c_update({ current: row.current ?? '' });
			default:
				return late
					? m.overview_upcoming_c_expired({ date: dueDate(row), monitor })
					: m.overview_upcoming_c_expires({ date: dueDate(row), monitor });
		}
	}

	/** "in 6 days", "~11 days" (a forecast), "3 days ago", "today", "now". */
	function due(row: UpcomingRow): string {
		if (row.kind === 'update') return m.overview_upcoming_due_available();
		if (row.kind === 'vulnerability') return m.overview_upcoming_due_now();
		const days = row.days ?? 0;
		if (days <= -1) return m.overview_upcoming_due_ago({ count: Math.ceil(-days) });
		if (days < 1) return m.overview_upcoming_due_today();
		if (row.kind === 'disk') return m.overview_upcoming_due_about({ count: Math.round(days) });
		return m.overview_upcoming_due_in({ count: Math.floor(days) });
	}

	const TONE: Record<UpcomingTone, string> = {
		urgent: 'text-warning-ink',
		soon: 'text-advisory-ink',
		calm: 'text-ink-2'
	};
</script>

<section class="flex min-w-0 flex-col overflow-hidden rounded-xl border border-line bg-surface" aria-labelledby="upcoming-title">
	<div class="flex items-baseline gap-x-2.5 px-4 pt-3.5 pb-3">
		<h2 id="upcoming-title" class="m-0 text-base font-semibold text-ink">{m.overview_upcoming_title()}</h2>
		{#if rows.length > ROW_LIMIT}
			<a href={withBase('/reports')} class="ml-auto inline-flex min-h-10 items-center gap-1 text-sm font-medium text-ink-2 hover:text-ink max-sm:-my-2">
				{m.overview_upcoming_see_all()}<ArrowRight class="size-3.5" aria-hidden="true" />
			</a>
		{/if}
	</div>

	{#if failed}
		<p class="m-0 border-t border-line px-4 py-3 text-sm text-ink-2" role="alert">{m.overview_upcoming_error()}</p>
	{:else if deadlines === null}
		<div class="border-t border-line px-4 py-3"><Skeleton class="h-16 w-full" /></div>
	{:else if rows.length === 0}
		<p class="state-in m-0 border-t border-line px-4 py-4 text-sm text-ink-2">{m.overview_upcoming_empty({ days: HORIZON_DAYS })}</p>
	{:else}
		<ul class="m-0 list-none p-0" aria-label={m.overview_upcoming_list_label()}>
			{#each shown as row (row.key)}
				<li class="border-t border-line">
					<a href={withBase(row.href)} class="flex min-h-10 items-start gap-3 px-4 py-2.5 hover:bg-surface-2">
						<span class="min-w-0 flex-1">
							<span class="block text-xs font-medium text-ink-2">{kindLabel(row)}</span>
							<span class="block truncate text-sm font-semibold text-ink">{title(row)}</span>
							<span class="block truncate text-sm text-ink-2" title={context(row)}>{context(row)}</span>
						</span>
						<span class={`shrink-0 pt-0.5 text-right font-mono text-sm font-medium whitespace-nowrap tabular-nums ${TONE[row.tone]}`}>{due(row)}</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</section>
