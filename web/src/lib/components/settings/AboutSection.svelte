<script lang="ts">
	/**
	 * Settings → About: version and the health of the two stores behind the
	 * server. Polled every 15 s while the page is open. Below, discreet, the
	 * first-run stopwatch: when this instance passed each step of the road to
	 * its first alert, kept on the instance only.
	 */
	import { BookOpen } from 'lucide-svelte';
	import { startPolling } from '#lib/poll.js';
	import { m } from '#lib/paraglide/messages.js';
	import { getHealth, getOnboarding, type ComponentHealth, type FirstRunTimings, type Health } from '#lib/api/index.js';
	import { formatDateTime, formatDuration, parseServerDate } from '#lib/format.js';
	import { firstRunSteps } from '#lib/components/wizard/quickstart.js';
	import { Button, ErrorNotice, Panel, Plate, Skeleton } from '#lib/ui/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { upgradeNotes } from '#lib/stores/upgradenotes.svelte.js';
	import { whatsNew } from '#lib/stores/whatsnew.svelte.js';
	import { releaseFor } from '#lib/whatsnew/releases.js';
	import UpdateNotice from '#lib/components/settings/UpdateNotice.svelte';

	let health = $state<Health | null>(null);
	let loading = $state(true);
	let error = $state<unknown>(null);

	async function load(signal?: AbortSignal) {
		error = null;
		try {
			health = await getHealth(signal);
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		return startPolling(load, 15_000);
	});

	// The configuration changes an update asked for (administrators): the list
	// itself is the window mounted by the root layout.
	const upgradeNotesShown = $derived(auth.user?.role === 'admin' && !auth.demo);
	$effect(() => {
		if (upgradeNotesShown) upgradeNotes.load({ quiet: true }).catch(() => {});
	});

	// The first-run stopwatch, read once: it only ever moves forward on a new step.
	let firstRun = $state<FirstRunTimings | null>(null);
	$effect(() => {
		const controller = new AbortController();
		getOnboarding(controller.signal)
			.then((state) => (firstRun = state.first_run ?? null))
			.catch(() => {});
		return () => controller.abort();
	});
	const firstRunLines = $derived(firstRunSteps(firstRun, parseServerDate));
	/** Seconds stay exact for the first ten minutes: that is the range the measure is about. */
	const elapsedLabel = (seconds: number) => (seconds < 600 ? m.misc_dur_s({ n: seconds }) : formatDuration(seconds));
	const FIRST_RUN_LABEL: Record<keyof FirstRunTimings, () => string> = {
		first_boot_at: () => m.settings_about_first_run_boot(),
		setup_at: () => m.settings_about_first_run_setup(),
		first_target_at: () => m.settings_about_first_run_target(),
		first_channel_at: () => m.settings_about_first_run_channel(),
		first_notification_at: () => m.settings_about_first_run_notification(),
		first_alert_at: () => m.settings_about_first_run_alert()
	};

	const COMPONENTS: { key: 'database' | 'victoria'; name: () => string; role: () => string }[] = [
		{ key: 'database', name: () => m.settings_about_database(), role: () => m.settings_about_database_role() },
		{ key: 'victoria', name: () => 'VictoriaMetrics', role: () => m.settings_about_victoria_role() }
	];
</script>

<Panel id="about" title={m.settings_about_title()}>
	{#if error}
		<ErrorNotice {error} title={m.settings_about_error_title()} onretry={() => void load()} />
	{:else if loading && !health}
		<Skeleton class="h-5 w-40" />
		<div class="mt-4 grid gap-2"><Skeleton class="h-12 w-full" rows={2} /></div>
	{:else if health}
		<dl class="grid gap-y-3 text-sm sm:grid-cols-[10rem_minmax(0,1fr)] sm:gap-x-6">
			<dt class="font-semibold text-ink">{m.settings_about_version()}</dt>
			<dd class="tnum text-ink-2">
				{health.version}{#if health.build}<span class="text-ink-3"> · {m.settings_about_build()} <span class="font-mono">{health.build}</span></span>{/if}
			</dd>

			{#each COMPONENTS as component (component.key)}
				{@const state: ComponentHealth = health[component.key]}
				<dt class="font-semibold text-ink">{component.name()}</dt>
				<dd class="min-w-0">
					<div class="flex flex-wrap items-center gap-2">
						{#if state.ok}
							<Plate tone="signal" label={m.settings_about_reporting()} />
						{:else}
							<Plate tone="warning" label={m.settings_about_warning()} />
						{/if}
						<span class="text-ink-2">{component.role()}</span>
					</div>
					{#if !state.ok}
						<p class="mt-1 break-words text-warning-ink">{state.error ?? m.settings_about_unreachable()}</p>
						<p class="mt-0.5 text-ink-2">{m.settings_about_check_logs()}</p>
					{/if}
				</dd>
			{/each}
		</dl>
	{/if}

	<UpdateNotice onSeeWhatsNew={() => whatsNew.reopen()} />

	{#if firstRunLines.length > 0}
		<details class="mt-5 border-t border-line pt-4 text-sm" data-testid="first-run">
			<summary class="flex min-h-10 cursor-pointer items-center font-semibold text-ink">{m.settings_about_first_run_title()}</summary>
			<p class="mt-1 max-w-[65ch] text-ink-2">{m.settings_about_first_run_text()}</p>
			<ol class="mt-3 grid gap-2">
				{#each firstRunLines as line, i (line.key)}
					<li class="grid gap-x-6 sm:grid-cols-[minmax(0,16rem)_8rem_minmax(0,1fr)]">
						<span class="text-ink">{FIRST_RUN_LABEL[line.key]()}</span>
						<span class="tnum text-ink-2">{i === 0 ? m.settings_about_first_run_start() : m.settings_about_first_run_after({ span: elapsedLabel(line.elapsed) })}</span>
						<time class="tnum text-ink-3" datetime={line.at.toISOString()}>{formatDateTime(line.at)}</time>
					</li>
				{/each}
			</ol>
		</details>
	{/if}

	{#if upgradeNotesShown && upgradeNotes.overview}
		<div class="mt-5 flex flex-wrap items-center justify-between gap-x-4 gap-y-3 border-t border-line pt-4 text-sm" data-testid="upgrade-notes-link">
			<div class="min-w-0 flex-1 basis-64">
				<h3 class="font-semibold text-ink">{m.upgrade_about_title()}</h3>
				<p class="mt-0.5 text-ink-2">{m.upgrade_about_text()}</p>
			</div>
			<div class="flex flex-wrap items-center gap-3">
				{#if upgradeNotes.pending > 0}
					<Plate tone="warning" label={m.upgrade_about_pending({ count: upgradeNotes.pending })} />
				{:else}
					<Plate tone="ghost" label={m.upgrade_about_none()} />
				{/if}
				<Button variant="secondary" size="sm" onclick={() => upgradeNotes.show('all')}>{m.upgrade_about_open()}</Button>
			</div>
		</div>
	{/if}

	<div class="mt-5 flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-line pt-4 text-sm">
		<a href="https://dumbmonit.selfhostr.io/docs/" target="_blank" rel="noopener" class="inline-flex items-center gap-1.5 font-medium text-signal-ink hover:underline">
			<BookOpen class="size-4" aria-hidden="true" />
			{m.settings_about_docs()}
		</a>
		{#if health && releaseFor(health.version)}
			<button type="button" class="font-medium text-signal-ink hover:underline" onclick={() => whatsNew.reopen()}>{m.settings_about_whats_new()}</button>
		{/if}
		<span class="text-ink-2">{m.settings_about_license()}</span>
	</div>
</Panel>
