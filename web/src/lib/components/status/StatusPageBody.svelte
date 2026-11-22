<script lang="ts">
	/**
	 * What a public status page shows, from one `PublicStatus` document: logo,
	 * title and the overall banner, the open announcements (incidents,
	 * maintenance) with their timeline, the services by group with their history
	 * bar, "Past incidents" by day for 30 days, how to get updates, the footer.
	 *
	 * Drawn by the public page (`PublicStatusPage`, which loads the document and
	 * owns the theme and the tab) and by the editor's live preview, which builds
	 * the document from unsaved settings: both show exactly the same thing.
	 * Nothing here names the product.
	 */
	import { tick, type Snippet } from 'svelte';
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import { CalendarClock, ExternalLink, Megaphone, RefreshCw } from 'lucide-svelte';
	import type { PublicIncident, PublicOlderIncidents, PublicStatus } from '#lib/api/index.js';
	import { formatDateTime, formatRelative, parseServerDate } from '#lib/format.js';
	import { Button, EmptyState, ErrorNotice, Plate } from '#lib/ui/index.js';
	import IncidentCard from './IncidentCard.svelte';
	import StatusUpdates from './StatusUpdates.svelte';
	import CalendarHistory from './CalendarHistory.svelte';
	import ServiceRow from './ServiceRow.svelte';
	import { periodParts } from './period';
	import { FILTER_THRESHOLD, filterGroups, incidentAnchor, incidentFromHash } from './incident-view';
	import { clockWithZone, zoneName } from './timezone';
	import { homepageHost, isClosed, overallBanner } from './words';

	interface Props {
		status: PublicStatus;
		/** When the document was last read; `null`: not said. */
		lastChecked?: Date | null;
		/** A history bar was opened on a day (`YYYY-MM-DD`). */
		onselectday?: (day: string) => void;
		/** Drawn under the services: the hour-by-hour detail of the opened day. */
		day?: Snippet;
		/** Reload the document now; absent (the editor's preview): no button. */
		onrefresh?: () => void;
		/** A reload is in flight. */
		refreshing?: boolean;
		/** Reads the next slice of incidents older than 30 days; absent: no "Show older" button. */
		loadOlder?: (cursor?: string) => Promise<PublicOlderIncidents>;
	}

	let { status, lastChecked = null, onselectday, day, onrefresh, refreshing = false, loadOlder }: Props = $props();

	const banner = $derived(overallBanner(status));
	const bannerTone = $derived(banner.tone);
	const days = $derived(status.page.history_days ?? 7);
	const windowSecs = $derived(status.page.window_secs ?? days * 86400);
	const stepSecs = $derived(status.page.step_secs ?? 86400);
	const periodLabel = $derived.by(() => {
		const part = periodParts(windowSecs);
		if (part.unit === 'hours') return part.count === 1 ? m.status_period_hour() : m.status_period_hours({ count: part.count });
		return m.status_period_days({ count: part.count });
	});

	// Open announcements sit at the top; everything closed goes to the history.
	const active = $derived<PublicIncident[]>([...status.maintenance, ...status.incidents].filter((i) => !isClosed(i.status)));
	const past = $derived<PublicIncident[]>([...status.incidents, ...status.maintenance].filter((i) => isClosed(i.status)));

	// "Show older": slices of resolved incidents beyond the 30 days of the document.
	let older = $state<PublicIncident[]>([]);
	let olderNext = $state<string | undefined>(undefined);
	let olderStarted = $state(false);
	let olderDone = $state(false);
	let olderLoading = $state(false);
	let olderError = $state<unknown>(null);

	async function showOlder() {
		if (!loadOlder || olderLoading) return;
		olderLoading = true;
		olderError = null;
		try {
			const slice = await loadOlder(olderStarted ? olderNext : undefined);
			const known = new Set([...past, ...older].map((i) => i.id));
			older = [...older, ...slice.incidents.filter((i) => !known.has(i.id))];
			olderNext = slice.next;
			olderStarted = true;
			olderDone = !slice.next;
		} catch (cause) {
			olderError = cause;
		} finally {
			olderLoading = false;
		}
	}

	// Services: a search and a "problems only" switch once the list is long.
	let query = $state('');
	let onlyProblems = $state(false);

	// "Past incidents" grouped by the day they started, newest day first.
	const pastByDay = $derived.by(() => {
		const dayFormat = new Intl.DateTimeFormat(getLocale(), { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
		const groups = new Map<string, { label: string; incidents: PublicIncident[] }>();
		for (const incident of [...past, ...older]) {
			const date = parseServerDate(incident.starts_at);
			const key = date ? date.toISOString().slice(0, 10) : incident.starts_at;
			const label = date ? dayFormat.format(date) : incident.starts_at;
			const group = groups.get(key) ?? { label, incidents: [] };
			group.incidents.push(incident);
			groups.set(key, group);
		}
		return [...groups.entries()].sort((a, b) => (a[0] < b[0] ? 1 : -1)).map(([, group]) => group);
	});

	/** Services a subscriber may follow, by public key. */
	const subscribeServices = $derived(status.groups.flatMap((g) => g.items.map((item) => ({ key: item.key, label: item.label }))));
	const serviceCount = $derived(status.groups.reduce((n, g) => n + g.items.length, 0));
	const filtering = $derived(serviceCount > FILTER_THRESHOLD);
	const shownGroups = $derived(filtering ? filterGroups(status.groups, query, onlyProblems) : status.groups);

	// A permalink (`#incident-<id>`) opened before the page was drawn: scroll to the card once it exists.
	let scrolledToHash = false;
	$effect(() => {
		void past;
		void active;
		if (scrolledToHash || typeof location === 'undefined') return;
		const id = incidentFromHash(location.hash);
		if (id === null) return;
		void tick().then(() => {
			const card = document.getElementById(incidentAnchor(id));
			if (!card) return;
			scrolledToHash = true;
			card.scrollIntoView({ block: 'start' });
		});
	});
</script>

<header class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
	<div class="flex min-w-0 items-center gap-3">
		{#if status.page.logo_url}
			<img src={status.page.logo_url} alt={status.page.brand_name ?? ''} class="size-9 shrink-0 rounded-md object-contain" />
		{/if}
		<div class="min-w-0">
			{#if status.page.brand_name && status.page.brand_name !== status.page.title}
				<p class="text-xs font-semibold text-ink-2 [overflow-wrap:anywhere]">{status.page.brand_name}</p>
			{/if}
			<h1 class="text-lg font-semibold tracking-tight break-words text-ink sm:text-xl">{status.page.title}</h1>
		</div>
	</div>
	{#if status.page.homepage_url}
		<a
			class="inline-flex min-h-10 items-center gap-1.5 rounded-md text-sm font-semibold text-accent underline decoration-accent/40 underline-offset-4 hover:decoration-accent"
			href={withBase(status.page.homepage_url)}
			rel="noopener noreferrer nofollow"
		>
			{homepageHost(status.page.homepage_url)}
			<ExternalLink class="size-3.5" aria-hidden="true" />
		</a>
	{/if}
</header>
{#if status.page.description}
	<p class="mt-1 max-w-prose text-sm text-ink-2">{status.page.description}</p>
{/if}

<!-- Overall banner: the one-second answer. -->
<section
	class={`mt-4 flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-[var(--radius-card)] border px-3 py-2.5 sm:px-4 ${bannerTone === 'signal' ? 'border-signal/30 bg-signal-soft' : bannerTone === 'advisory' ? 'border-advisory/35 bg-advisory-soft' : bannerTone === 'warning' ? 'border-warning/35 bg-warning-soft' : bannerTone === 'ghost' ? 'border-line bg-surface' : 'border-info/30 bg-info-soft'}`}
	aria-live="polite"
	aria-label={m.status_public_current_status()}
>
	<div class="flex min-w-0 items-center gap-2.5">
		<Plate tone={bannerTone} label={banner.plate} />
		<p class="text-base font-semibold tracking-tight text-ink">{banner.label}</p>
	</div>
	<div class="flex flex-wrap items-center gap-x-2 text-xs text-ink-2">
		{#if lastChecked}
			<p>
				{m.status_public_checked()} <time class="tnum" datetime={lastChecked.toISOString()} title={formatDateTime(lastChecked)}>{formatRelative(lastChecked)}</time>
				<span class="tnum text-ink-3" title={zoneName()}>· {clockWithZone(lastChecked, getLocale())}</span>
			</p>
		{/if}
		{#if onrefresh}
			<button
				type="button"
				class="inline-flex min-h-10 items-center gap-1.5 rounded-md px-2 font-semibold text-ink-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:opacity-60"
				onclick={onrefresh}
				disabled={refreshing}
			>
				<RefreshCw class={`size-3.5 ${refreshing ? 'motion-safe:animate-spin' : ''}`} aria-hidden="true" />
				{m.status_public_refresh()}
			</button>
		{/if}
	</div>
</section>

{#if active.length > 0}
	<section class="mt-5" aria-labelledby="announcements">
		<h2 id="announcements" class="text-sm font-semibold text-ink">
			{active.length === 1 ? m.status_public_announcement_one() : m.status_public_announcement_many()}
		</h2>
		<div class="mt-2 grid gap-2">
			{#each active as incident (incident.id)}
				<IncidentCard {incident} />
			{/each}
		</div>
	</section>
{/if}

<section class="mt-5" aria-labelledby="services">
	<div class="flex items-baseline justify-between gap-3">
		<h2 id="services" class="text-sm font-semibold text-ink">{m.status_public_services()}</h2>
		<p class="text-xs text-ink-3">{stepSecs < 86400 ? periodLabel : m.status_public_uptime_last({ days })}</p>
	</div>
	{#if serviceCount === 0}
		<EmptyState class="mt-2" icon={Megaphone} title={m.status_public_no_service_title()} description={m.status_public_no_service_description()} />
	{:else}
		{#if filtering}
			<div class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
				<label class="sr-only" for="service-search">{m.status_public_filter_label()}</label>
				<input id="service-search" type="search" class="input min-w-0 flex-1 sm:max-w-xs" bind:value={query} placeholder={m.status_public_filter_placeholder()} autocomplete="off" />
				<label class="inline-flex min-h-10 items-center gap-2 text-sm text-ink">
					<input type="checkbox" class="size-4 accent-[var(--c-signal)]" bind:checked={onlyProblems} />
					{m.status_public_filter_problems()}
				</label>
			</div>
		{/if}
		{#if shownGroups.length === 0}
			<p class="mt-3 text-sm text-ink-2" role="status">{m.status_public_filter_none()}</p>
		{/if}
		<div class="mt-2 grid gap-3">
			{#each shownGroups as group (group.name)}
				<div class="rounded-[var(--radius-card)] border border-line bg-surface">
					{#if group.name}
						<h3 class="border-b border-line px-3 py-1.5 text-xs font-semibold text-ink-2 sm:px-4">{group.name}</h3>
					{/if}
					<ul class="divide-y divide-line" role="list">
						{#each group.items as item (item.key)}
							<ServiceRow {item} {days} {windowSecs} {stepSecs} {periodLabel} onselectday={onselectday} />
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	{/if}
	{@render day?.()}
	{#if status.calendar && status.calendar.length > 0}
		<h2 class="mt-5 text-sm font-semibold text-ink">{m.status_calendar_title({ days: status.calendar.length })}</h2>
		<div class="mt-2"><CalendarHistory days={status.calendar} /></div>
	{/if}
</section>

<section class="mt-6" aria-labelledby="past">
	<div class="flex items-baseline justify-between gap-3">
		<h2 id="past" class="text-sm font-semibold text-ink">{m.status_public_past()}</h2>
		<p class="text-xs text-ink-3">{m.status_public_last_30()}</p>
	</div>
	{#if pastByDay.length === 0}
		<EmptyState class="mt-2" icon={CalendarClock} title={m.status_public_no_incident()} tone="signal" />
	{:else}
		<div class="mt-2 grid gap-3">
			{#each pastByDay as group (group.label)}
				<div>
					<h3 class="border-b border-line pb-1 text-xs font-semibold text-ink-2">{group.label}</h3>
					<div class="mt-1.5 grid gap-1.5">
						{#each group.incidents as incident (incident.id)}
							<IncidentCard {incident} compact />
						{/each}
					</div>
				</div>
			{/each}
		</div>
	{/if}
	{#if loadOlder}
		{#if olderError}
			<ErrorNotice class="mt-3" error={olderError} title={m.status_public_load_error()} onretry={() => void showOlder()} />
		{:else if olderDone}
			<p class="mt-3 text-center text-xs text-ink-3" role="status">{older.length > 0 ? m.status_public_older_end() : m.status_public_older_none()}</p>
		{:else}
			<div class="mt-3 flex justify-center">
				<Button variant="secondary" size="sm" loading={olderLoading} onclick={() => void showOlder()}>{m.status_public_older_show()}</Button>
			</div>
		{/if}
	{/if}
</section>

<StatusUpdates slug={status.page.slug} kinds={status.page.subscribe_kinds ?? []} services={subscribeServices} />

{#if status.page.footer_text}
	<footer class="mt-8 text-center text-xs text-ink-3">
		<p class="mx-auto max-w-prose whitespace-pre-line">{status.page.footer_text}</p>
	</footer>
{/if}
