<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * Overview: one screen, without scrolling the page on a wide window (see
	 * `overview/dash/Dashboard.svelte`): three figures, what to act on now,
	 * what is coming up, the recent events, the service checks and the fleet
	 * by site. A fresh instance gets the first-run guide (administrators) or a
	 * way in instead.
	 *
	 * Monitors, probes and alerts come from the app-wide store (polled every
	 * 30 s); the dashboard's own figures from the widgets store (one request,
	 * every minute); the alert history from the fleet store.
	 */
	import { untrack } from 'svelte';
	import { getOnboarding, listCollectors, type OnboardingState } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { consoleState } from '#lib/stores/console.svelte.js';
	import { Button, EmptyState, ErrorNotice } from '#lib/ui/index.js';
	import FirstRun from '#lib/components/overview/FirstRun.svelte';
	import Dashboard from '#lib/components/overview/dash/Dashboard.svelte';
	import { widgetsStore } from '#lib/components/overview/dash/widgets.svelte.js';
	import { fleetStore } from './fleet.svelte.js';

	// One subscription for the life of the view; the range is followed separately.
	$effect(() => untrack(() => fleetStore.start()));
	$effect(() => untrack(() => widgetsStore.start()));
	// A new range reloads the figures at once (the first load comes with `start`).
	$effect(() => {
		consoleState.range;
		fleetStore.rangeChanged();
	});
	let widgetsRange = untrack(() => consoleState.range);
	$effect(() => {
		const range = consoleState.range;
		const changed = range !== widgetsRange;
		widgetsRange = range;
		if (changed) untrack(() => void widgetsStore.refresh());
	});

	// --- Empty state -------------------------------------------------------------
	/** Nothing read yet: the line waits rather than say "nothing to watch". */
	const reading = $derived(alertsStore.loading && alertsStore.targets.length === 0);
	const noMonitors = $derived(!reading && alertsStore.available && alertsStore.targets.length === 0);

	// --- First run -----------------------------------------------------------------
	// The three-step guide, as the server keeps it. Decided once per visit and
	// then held, so the last step turning green does not make it vanish.
	// Silent on failure: no guide is better than an error over a working console.
	let onboarding = $state<OnboardingState | null>(null);
	let showGuide = $state(false);
	let hasDemo = $state(false);
	$effect(() => {
		// A viewer can neither add a device nor create a channel.
		if (!auth.isAdmin) return;
		const controller = new AbortController();
		getOnboarding(controller.signal)
			.then((next) => {
				onboarding = next;
				showGuide = !next.skipped && !next.complete;
				if (!showGuide) return;
				void listCollectors(controller.signal)
					.then((kinds) => (hasDemo = kinds.some((kind) => kind.kind === 'dummy')))
					.catch(() => {});
			})
			.catch(() => {});
		return () => controller.abort();
	});

	/** "Or [install the agent on a machine], or [watch a remote site] through one." cut around its two links. */
	const orParts = $derived(
		m
			.overview_page_or({
				agent: '\u0000' + m.overview_page_or_agent() + '\u0000',
				relay: '\u0000' + m.overview_page_or_relay() + '\u0000'
			})
			.split('\u0000')
	);
</script>

<div class="flex min-h-0 flex-1 flex-col gap-4">
	<h1 class="sr-only">{m.misc_nav_overview()}</h1>

	{#if showGuide && onboarding}
		<!-- The guide replaces the console until it is done: one thing to do at a time. -->
		<FirstRun
			guide={onboarding}
			{hasDemo}
			onchange={(next) => {
				onboarding = next;
				if (next.skipped) showGuide = false;
			}}
		/>
	{:else if !alertsStore.available && alertsStore.targets.length === 0 && alertsStore.lastError}
		<ErrorNotice error={alertsStore.lastError} title={m.overview_page_error_load()} onretry={() => void alertsStore.refresh()} />
	{:else if noMonitors}
		<EmptyState title={m.overview_page_empty_title()} description={m.overview_page_empty_text()} mascot="watch">
			{#snippet action()}
				<Button variant="primary" href="/targets/new">{m.overview_first_device_action()}</Button>
				{#if auth.isAdmin}
					<p class="mt-4 text-sm text-ink-2">
						{orParts[0]}<a href={withBase('/targets/new?kind=agent')} class="inline-block py-1 font-semibold text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink">{orParts[1]}</a>{orParts[2]}<a href={withBase('/targets/new?kind=agent&via=relay')} class="inline-block py-1 font-semibold text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink">{orParts[3]}</a>{orParts[4]}
					</p>
				{/if}
			{/snippet}
		</EmptyState>
	{:else}
		<Dashboard />
	{/if}
</div>
