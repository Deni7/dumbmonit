<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Maintenance over the next seven days: a strip per site with each window
	 * placed on it and a line at the present (wide cards only), then the next
	 * windows and the monitors paused by hand. Windows and their occurrences
	 * are read every five minutes; sites come from the dashboards' store,
	 * paused monitors from the app-wide store.
	 */
	import { listSilenceOccurrences, listSilences, type Silence, type SilenceOccurrence } from '#lib/api/index.js';
	import { parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { startPolling } from '#lib/poll.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { maintenanceStrip, nextOccurrences } from '../parts/timeline.js';
	import { siteIndex } from '../parts/sites.js';
	import { durationText, soonText } from '../parts/words.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 4 }: { span?: Span } = $props();

	const DAYS = 7;
	const REFRESH_MS = 5 * 60_000;
	/** Paused monitors listed under the windows; the rest is on Monitors. */
	const PAUSED_LIMIT = 3;

	function midnight(): number {
		const d = new Date();
		d.setHours(0, 0, 0, 0);
		return d.getTime();
	}

	let occurrences = $state<SilenceOccurrence[] | null>(null);
	let silences = $state<Silence[]>([]);
	let error = $state<unknown>(null);
	let from = $state(midnight());
	let now = $state(Date.now());

	$effect(() =>
		startPolling(async (signal) => {
			const start = midnight();
			try {
				const [occ, list] = await Promise.all([
					listSilenceOccurrences(new Date(start).toISOString(), new Date(start + DAYS * 86400_000).toISOString(), signal),
					listSilences(signal)
				]);
				occurrences = occ;
				silences = list;
				from = start;
				now = Date.now();
				error = null;
				return true;
			} catch (cause) {
				if (signal.aborted) return true;
				error = cause;
				return false;
			}
		}, REFRESH_MS)
	);
	// The "now" line moves on between two reads.
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 60_000);
		return () => clearInterval(timer);
	});

	const sites = $derived(widgetsStore.data?.sites ?? []);
	const strip = $derived(
		maintenanceStrip({ occurrences: occurrences ?? [], silences, sites, from, now, days: DAYS, fallback: m.w9dash_common_this_site() })
	);
	const days = $derived(
		Array.from({ length: DAYS }, (_, i) => {
			const d = new Date(from);
			d.setDate(d.getDate() + i);
			return { key: i, label: d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' }) };
		})
	);
	const next = $derived(nextOccurrences(occurrences ?? [], now, 3));
	const places = $derived(siteIndex(sites));
	const paused = $derived(alertsStore.targets.filter((t) => !t.enabled).slice(0, PAUSED_LIMIT));

	function lasts(occ: SilenceOccurrence): string {
		const start = parseServerDate(occ.starts_at)?.getTime() ?? 0;
		const end = parseServerDate(occ.ends_at)?.getTime() ?? 0;
		const duration = durationText(Math.max(0, (end - start) / 1000));
		return occ.recurring ? m.w9dash_maint_lasts_recurring({ duration }) : m.w9dash_maint_lasts({ duration });
	}
</script>

<WidgetCard title={m.w9dash_maint_title()} sub={m.w9dash_maint_sub({ days: DAYS })} {span}>
	{#snippet right()}
		{#if auth.canSilence}
			<Button size="sm" variant="secondary" href="/alerts?tab=now&maintenance=1">{m.w9dash_maint_schedule()}</Button>
		{/if}
	{/snippet}
	{#if occurrences === null && !error}
		<Skeleton class="h-7 w-full" rows={3} />
	{:else if occurrences === null}
		<ErrorNotice {error} title={m.w9dash_maint_error()} compact />
	{:else}
		<div class="@container">
			<!-- The strip needs room for seven days: narrow cards keep only the list. -->
			<div class="relative mt-1 hidden @lg:block" role="img" aria-label={m.w9dash_maint_strip_label({ days: DAYS })}>
				<div class="grid grid-cols-[4.75rem_repeat(7,minmax(0,1fr))] text-[0.71875rem] text-ink-3" aria-hidden="true">
					<span></span>
					{#each days as day (day.key)}
						<span class="truncate border-l border-line pb-1 pl-1">{day.label}</span>
					{/each}
				</div>
				{#each strip.rows as row, ri (row.key)}
					<div class="grid min-h-[30px] grid-cols-[4.75rem_minmax(0,1fr)] items-center border-t border-line">
						<b class="truncate pr-1 text-xs font-semibold text-ink-2">{row.name}</b>
						<div class="relative h-[30px] bg-[repeating-linear-gradient(90deg,transparent_0,transparent_calc(100%/7_-_1px),var(--color-line)_calc(100%/7_-_1px),var(--color-line)_calc(100%/7))]">
							{#each row.windows as w (w.key)}
								<span
									class={`absolute top-1.5 h-[18px] min-w-2 rounded-[5px] border border-info ${w.active ? 'bg-info/40' : 'bg-info-soft'}`}
									style={`left:${w.left.toFixed(2)}%;width:${w.width.toFixed(2)}%`}
									title={w.name}
								></span>
							{/each}
							<span class="pointer-events-none absolute top-0 bottom-0 w-0 border-l-2 border-ink" style={`left:${strip.nowPct.toFixed(2)}%`}>
								{#if ri === 0}<span class="absolute -top-[17px] left-1 text-[0.6875rem] font-bold text-ink">{m.w9dash_maint_now()}</span>{/if}
							</span>
						</div>
					</div>
				{/each}
			</div>

			{#if next.length === 0 && paused.length === 0}
				<p class="m-0 py-2 text-sm text-ink-2">{m.w9dash_maint_empty({ days: DAYS })}</p>
			{:else}
				<ul class="m-0 mt-2 list-none p-0">
					{#each next as occ, i (`${occ.silence_id}-${occ.starts_at}`)}
						{@const running = occ.state === 'active'}
						<li class={`flex min-w-0 items-center gap-2.5 py-2 ${i > 0 ? 'border-t border-line' : ''}`}>
							<Plate tone="info" label={running ? m.w9dash_maint_running() : soonText(occ.starts_at)} class="shrink-0" />
							<div class="min-w-0 flex-1">
								<b class="block truncate text-sm font-semibold text-ink">{occ.name}</b>
								<span class="block truncate text-[0.8125rem] text-ink-2">{lasts(occ)}</span>
							</div>
						</li>
					{/each}
					{#each paused as target, i (target.id)}
						{@const place = places.get(target.id)}
						<li class={`flex min-w-0 items-center gap-2.5 py-2 ${i > 0 || next.length > 0 ? 'border-t border-line' : ''}`}>
							<Plate tone="muted" label={m.w9dash_status_paused()} class="shrink-0" />
							<div class="min-w-0 flex-1">
								<a href={withBase(`/targets/${target.id}`)} class="block truncate text-sm font-semibold text-ink hover:underline">{target.name}</a>
								{#if place}<span class="block truncate text-[0.8125rem] text-ink-2">{m.w9dash_problems_place({ kind: place.kind, site: place.siteName })}</span>{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</WidgetCard>
