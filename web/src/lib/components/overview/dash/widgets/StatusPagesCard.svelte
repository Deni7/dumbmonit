<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The status pages and what each one tells its readers right now:
	 * operational, a partial or major outage (a monitor it shows is down, or an
	 * incident is open), or maintenance in progress. Who can read it and how
	 * many services it shows; during an outage, a way to post an update.
	 * Pages and incidents are read every two minutes; the monitors' states come
	 * from the app-wide store, so an outage shows as soon as it happens.
	 */
	import { listIncidents, listStatusPages, type Incident, type StatusPage } from '#lib/api/index.js';
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { consoleStatus } from '#lib/components/enterprise/status.js';
	import { m } from '#lib/paraglide/messages.js';
	import { startPolling } from '#lib/poll.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ErrorNotice, Plate, Skeleton, type Tone } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 4 }: { span?: Span } = $props();

	const REFRESH_MS = 2 * 60_000;
	const CLOSED = new Set(['resolved', 'completed']);

	let pages = $state<StatusPage[] | null>(null);
	let incidents = $state<Incident[]>([]);
	let error = $state<unknown>(null);

	$effect(() =>
		startPolling(async (signal) => {
			try {
				const [list, open] = await Promise.all([
					listStatusPages(signal),
					listIncidents(signal).catch(() => [] as Incident[])
				]);
				pages = list;
				incidents = open.filter((i) => !CLOSED.has(i.status));
				error = null;
				return true;
			} catch (cause) {
				if (signal.aborted) return true;
				error = cause;
				return false;
			}
		}, REFRESH_MS)
	);

	const alertCounts = $derived(countAlertsByTarget(alertsStore.alerts));
	const statusOf = $derived(
		new Map(
			alertsStore.targets.map((t) => [t.id, consoleStatus(t, alertsStore.probes.get(t.id), alertCounts.get(t.id))])
		)
	);

	type State = 'operational' | 'partial' | 'major' | 'maintenance';
	const TONE: Record<State, Tone> = { operational: 'signal', partial: 'advisory', major: 'warning', maintenance: 'info' };

	function stateOf(page: StatusPage): State {
		const watched = page.items.map((item) => statusOf.get(item.target_id)).filter((s) => s !== undefined && s !== 'paused');
		const down = watched.filter((s) => s === 'down').length;
		if (down > 0) return down === watched.length ? 'major' : 'partial';
		const mine = incidents.filter((i) => i.page_id === null || i.page_id === page.id);
		const open = mine.filter((i) => i.kind === 'incident');
		if (open.some((i) => i.severity === 'major')) return 'major';
		if (open.length > 0) return 'partial';
		if (mine.some((i) => i.kind === 'maintenance' && i.status === 'in_progress')) return 'maintenance';
		return 'operational';
	}

	function stateWord(state: State): string {
		switch (state) {
			case 'major':
				return m.w9dash_pages_major();
			case 'partial':
				return m.w9dash_pages_partial();
			case 'maintenance':
				return m.w9dash_pages_maintenance();
			default:
				return m.w9dash_pages_operational();
		}
	}

	function audience(page: StatusPage): string {
		if (!page.published) return m.w9dash_pages_draft({ count: page.items.length });
		if (page.access_mode === 'password') return m.w9dash_pages_password({ count: page.items.length });
		if (page.access_mode === 'users') return m.w9dash_pages_users({ count: page.items.length });
		return m.w9dash_pages_public({ count: page.items.length });
	}

	const rows = $derived((pages ?? []).map((page) => ({ page, state: stateOf(page) })));
</script>

<WidgetCard title={m.w9dash_pages_title()} {span}>
	{#if pages === null && !error}
		<Skeleton class="h-9 w-full" rows={2} />
	{:else if pages === null}
		<ErrorNotice {error} title={m.w9dash_pages_error()} compact />
	{:else if rows.length === 0}
		<p class="m-0 py-2 text-sm text-ink-2">{m.w9dash_pages_empty()}</p>
		{#if auth.isAdmin}
			<Button size="sm" variant="secondary" href="/status">{m.w9dash_pages_create()}</Button>
		{/if}
	{:else}
		<ul class="m-0 list-none p-0">
			{#each rows as { page, state }, i (page.id)}
				<li class={`flex min-w-0 items-center gap-2.5 py-2 ${i > 0 ? 'border-t border-line' : ''}`}>
					<Plate tone={TONE[state]} label={stateWord(state)} class="shrink-0" />
					<div class="min-w-0 flex-1">
						<a href={withBase(`/status/${page.id}`)} class="block truncate text-sm font-semibold text-ink hover:underline">{page.domain || page.title}</a>
						<span class="block truncate text-[0.8125rem] text-ink-2">{audience(page)}</span>
					</div>
					{#if (state === 'partial' || state === 'major') && auth.canOperate}
						<Button size="sm" variant="secondary" href={`/status/${page.id}`} class="shrink-0">{m.w9dash_pages_post()}</Button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet foot()}
		<a href={withBase('/status')}>{m.w9dash_pages_all()}</a>
	{/snippet}
</WidgetCard>
