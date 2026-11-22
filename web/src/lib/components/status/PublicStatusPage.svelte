<script lang="ts">
	/**
	 * Public status page. Rendered by `/s/<slug>`, and by the root layout at `/`
	 * when the request came in on the page's own public domain. No session, no
	 * nav, no cookie: one document from `GET /api/public/status/<slug>`,
	 * refreshed every minute, drawn by `StatusPageBody` (the editor's preview
	 * draws the same body from unsaved settings).
	 *
	 * This shell owns what surrounds the page: the theme follows the page setting
	 * (`light` / `dark`) or the visitor's system, in the console's own tokens;
	 * the accent is one of a closed set of tokens, never free-form CSS; the tab
	 * carries the page's title and, for a white-label page (`hide_branding`), its
	 * logo as icon or none, never the product's.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { startPolling } from '#lib/poll.js';
	import { FileQuestion } from 'lucide-svelte';
	import { getPublicOlderIncidents, getPublicStatus, getPublicStatusDay, toApiError, type PublicDayDetail, type PublicStatus } from '#lib/api/index.js';
	import { theme } from '#lib/stores/theme.svelte.js';
	import { EmptyState, ErrorNotice, Skeleton } from '#lib/ui/index.js';
	import AccessGate from '#lib/components/status/AccessGate.svelte';
	import DayDetail from '#lib/components/status/DayDetail.svelte';
	import StatusPageBody from '#lib/components/status/StatusPageBody.svelte';
	import { accentClass } from '#lib/components/status/words.js';

	const REFRESH_MS = 60_000;

	interface Props {
		slug: string;
	}

	let { slug }: Props = $props();

	let status = $state<PublicStatus | null>(null);
	const seo = $derived(status?.page.seo);
	// The server wrote these tags into the HTML for crawlers; once the page is drawn
	// the <svelte:head> below owns them, so the copies are dropped (no duplicates).
	$effect(() => {
		if (seo) document.head.querySelectorAll('[data-dm-seo]').forEach((node) => node.remove());
	});
	let loading = $state(true);
	let error = $state<unknown>(null);
	let lastChecked = $state<Date | null>(null);
	let refreshing = $state(false);

	async function load(signal?: AbortSignal) {
		refreshing = true;
		try {
			status = await getPublicStatus(slug, signal);
			error = null;
			lastChecked = new Date();
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			// A failed refresh keeps the last good document on screen.
			if (!status) error = cause;
		} finally {
			loading = false;
			refreshing = false;
		}
	}

	$effect(() => {
		const current = slug;
		if (!current) return;
		loading = true;
		status = null;
		return startPolling(load, REFRESH_MS);
	});

	// The page decides its light: a fixed theme overrides whatever the visitor
	// (or the admin, on this browser) chose; `auto` follows the system.
	$effect(() => {
		const wanted = status?.page.theme ?? 'auto';
		theme.apply(wanted === 'auto' ? undefined : wanted);
		return () => theme.apply();
	});

	// White label: the tab shows the page's logo, or no icon at all, and a phone
	// shortcut takes the page's name. The server already serves the HTML that way
	// (`status_seo::apply`); this keeps it so after the document is read, and
	// puts the product's icon back when leaving for the console.
	const whiteIcon = $derived(status?.page.hide_branding ? (status.page.logo_url ?? 'data:,') : null);
	const whiteName = $derived(status?.page.hide_branding ? status.page.title : null);
	$effect(() => {
		if (whiteIcon === null) return;
		const links = [...document.head.querySelectorAll<HTMLLinkElement>('link[rel="icon"], link[rel="apple-touch-icon"]')];
		const before = links.map((link) => link.getAttribute('href'));
		for (const link of links) link.setAttribute('href', whiteIcon);
		let added: HTMLLinkElement | null = null;
		if (!links.some((link) => link.rel === 'icon')) {
			added = document.createElement('link');
			added.rel = 'icon';
			added.href = whiteIcon;
			document.head.appendChild(added);
		}
		return () => {
			links.forEach((link, index) => {
				const href = before[index];
				if (href !== null) link.setAttribute('href', href);
			});
			added?.remove();
		};
	});
	$effect(() => {
		if (whiteName === null) return;
		const meta = document.head.querySelector<HTMLMetaElement>('meta[name="apple-mobile-web-app-title"]');
		if (!meta) return;
		const before = meta.content;
		meta.content = whiteName;
		return () => {
			meta.content = before;
		};
	});

	const notFound = $derived(error !== null && toApiError(error).status === 404);
	// A private page answers 401 and says what it wants: a password or a sign-in.
	const gate = $derived(error !== null ? toApiError(error).access : null);

	// Hour-by-hour detail of the day whose bar was opened.
	let openDay = $state<string | null>(null);
	let dayDetail = $state<PublicDayDetail | null>(null);
	let dayLoading = $state(false);
	let dayError = $state<unknown>(null);

	async function showDay(date: string) {
		openDay = date;
		dayDetail = null;
		dayError = null;
		dayLoading = true;
		try {
			const detail = await getPublicStatusDay(slug, date);
			if (openDay === date) dayDetail = detail;
		} catch (cause) {
			if (openDay === date) dayError = cause;
		} finally {
			if (openDay === date) dayLoading = false;
		}
	}
</script>

<svelte:head>
	<title>{seo ? seo.title : status ? m.status_public_title_with_page({ title: status.page.title }) : m.status_public_title()}</title>
	<!-- Same values the server wrote into the HTML (crates/server/src/status_seo.rs); noindex until the page says otherwise. -->
	<meta name="robots" content={seo?.indexable ? 'index, follow' : 'noindex, nofollow'} />
	{#if seo}
		<meta name="description" content={seo.description} />
		{#if seo.keywords}<meta name="keywords" content={seo.keywords} />{/if}
		{#if seo.indexable && seo.canonical}<link rel="canonical" href={seo.canonical} />{/if}
		<meta property="og:type" content="website" />
		<meta property="og:title" content={seo.title} />
		<meta property="og:description" content={seo.description} />
		<meta property="og:site_name" content={seo.site_name} />
		{#if seo.canonical}<meta property="og:url" content={seo.canonical} />{/if}
		{#if seo.image}<meta property="og:image" content={seo.image} />{/if}
		<meta name="twitter:card" content={seo.image ? 'summary_large_image' : 'summary'} />
		<meta name="twitter:title" content={seo.title} />
		<meta name="twitter:description" content={seo.description} />
		{#if seo.image}<meta name="twitter:image" content={seo.image} />{/if}
	{/if}
</svelte:head>

{#snippet dayPanel()}
	{#if openDay}
		<DayDetail date={openDay} detail={dayDetail} loading={dayLoading} error={dayError} onclose={() => (openDay = null)} />
	{/if}
{/snippet}

<div class={`min-h-full bg-canvas text-ink border-t-2 border-accent ${accentClass(status?.page.accent)}`}>
	<main class="mx-auto w-full max-w-3xl px-4 pt-5 pb-10 sm:px-6 sm:pt-8">
		{#if error && notFound}
			<EmptyState icon={FileQuestion} title={m.status_public_not_found_title()} description={m.status_public_not_found_description()} />
		{:else if error && gate}
			<AccessGate {slug} access={gate} onunlocked={() => { error = null; loading = true; void load(); }} />
		{:else if error}
			<ErrorNotice {error} title={m.status_public_load_error()} onretry={() => void load()} />
		{:else if loading || !status}
			<div class="grid gap-4" aria-busy="true" aria-label={m.status_public_loading()}>
				<Skeleton class="h-7 w-2/3" />
				<Skeleton class="h-12 w-full" />
				<Skeleton class="h-24 w-full" />
				<Skeleton class="h-24 w-full" />
			</div>
		{:else}
			<StatusPageBody
				{status}
				{lastChecked}
				{refreshing}
				onrefresh={() => void load()}
				loadOlder={(cursor) => getPublicOlderIncidents(slug, cursor)}
				onselectday={showDay}
				day={dayPanel}
			/>
		{/if}
	</main>
</div>
