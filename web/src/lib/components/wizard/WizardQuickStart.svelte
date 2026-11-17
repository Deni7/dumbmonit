<script lang="ts">
	/**
	 * Setup wizard, first step "Your first alert": the shortest road from a
	 * fresh instance to a message on the phone.
	 *
	 * 1. One field says what to watch (an URL, a host, `host:port`): the monitor
	 *    is created at once and the server probes it right away.
	 * 2. One click picks where the alert goes (ntfy, Discord, Telegram, email,
	 *    webhook, or any other kind of the catalogue): the channel form opens on
	 *    that kind, with an ntfy topic already chosen.
	 * 3. One button sends a test through that channel, the proof the chain works.
	 *
	 * The rail on the left fills as each link is proven, in words as well as
	 * with the dot. Everything else (tags, interval, parent, delivery policy)
	 * keeps its default and stays in the full forms.
	 */
	import { AlertTriangle, Check, CircleAlert, CircleCheck, CircleDashed, ExternalLink } from 'lucide-svelte';
	import {
		createTarget,
		getTarget,
		listChannelKinds,
		listChannels,
		listCollectors,
		listTargets,
		testChannel,
		type Channel,
		type Target
	} from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { Button, ErrorNotice, Skeleton } from '#lib/ui/index.js';
	import ChannelForm from '#lib/components/notifications/ChannelForm.svelte';
	import { kindIcon, normalizeKind, type KindInfo } from '#lib/components/notifications/kinds.js';
	import { expectedAlertMinutes, guessMonitor, ntfySubscribeUrl, ntfyTopic, quickKinds } from './quickstart.js';

	let loading = $state(true);
	let loadError = $state<unknown>(null);
	let collectorKinds = $state<string[]>([]);
	let kinds = $state<KindInfo[]>([]);
	let existingTargets = $state(0);
	let channels = $state<Channel[]>([]);

	$effect(() => {
		const controller = new AbortController();
		Promise.all([
			listCollectors(controller.signal),
			listChannelKinds(controller.signal),
			listChannels(controller.signal),
			listTargets(controller.signal)
		])
			.then(([collectors, catalogue, list, targets]) => {
				collectorKinds = collectors.map((c) => c.kind);
				kinds = (Array.isArray(catalogue) ? catalogue : []).filter((k) => k && typeof k.kind === 'string').map(normalizeKind);
				channels = (Array.isArray(list) ? list : []).filter((c) => c.enabled);
				existingTargets = Array.isArray(targets) ? targets.length : 0;
			})
			.catch((cause) => {
				if (!(cause instanceof DOMException && cause.name === 'AbortError')) loadError = cause;
			})
			.finally(() => (loading = false));
		return () => controller.abort();
	});

	// --- 1. What to watch ------------------------------------------------------

	let address = $state('');
	let addressError = $state<string | null>(null);
	let creating = $state(false);
	let createError = $state<unknown>(null);
	let watched = $state<Target | null>(null);
	/** Id of the monitor created here: the polling below follows it, not each re-read. */
	let watchedId = $state<number | null>(null);
	/** `probing` until the server's first probe is recorded, `slow` past 40 s. */
	let probe = $state<'probing' | 'done' | 'slow'>('probing');

	async function watch(event: SubmitEvent) {
		event.preventDefault();
		createError = null;
		const payload = guessMonitor(address, collectorKinds);
		if (!payload) {
			addressError = m.wizard_start_watch_invalid();
			return;
		}
		creating = true;
		try {
			watched = await createTarget(payload);
			probe = 'probing';
			watchedId = watched.id;
			address = '';
		} catch (cause) {
			createError = cause;
		} finally {
			creating = false;
		}
	}

	// The server probes a new monitor at once: read it back until the first result.
	$effect(() => {
		const id = watchedId;
		if (id === null) return;
		const started = Date.now();
		let stopped = false;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const poll = async () => {
			try {
				const fresh = await getTarget(id);
				if (stopped) return;
				watched = fresh;
				if (fresh.last_probe_at) {
					probe = 'done';
					return;
				}
			} catch {
				// A read that fails is retried at the next turn.
			}
			if (stopped) return;
			if (Date.now() - started > 40_000) {
				probe = 'slow';
				return;
			}
			timer = setTimeout(poll, 1500);
		};
		timer = setTimeout(poll, 800);
		return () => {
			stopped = true;
			clearTimeout(timer);
		};
	});

	const watchDone = $derived(watched !== null || existingTargets > 0);

	// --- 2. Where the alert goes ---------------------------------------------------

	const quick = $derived(quickKinds(kinds));
	/** Kind whose form is open; `''` opens the full picker; `null`: no form. */
	let picking = $state<string | null>(null);
	let presetSettings = $state<Record<string, string>>({});
	/** The channel set up here, or the first enabled one already there. */
	let chosen = $state<Channel | null>(null);
	const channel = $derived(chosen ?? channels[0] ?? null);

	function pick(kind: string) {
		presetSettings = kind === 'ntfy' ? { topic: ntfyTopic() } : {};
		picking = kind;
		result = null;
	}

	function saved(next: Channel) {
		channels = [...channels.filter((c) => c.id !== next.id), next];
		chosen = next;
		picking = null;
	}

	const ntfyLink = $derived(channel?.kind === 'ntfy' ? ntfySubscribeUrl(channel.settings?.server_url, channel.settings?.topic) : null);
	const channelDone = $derived(channel !== null);

	// --- 3. The proof ----------------------------------------------------------------

	let testing = $state(false);
	let result = $state<{ ok: boolean; message: string } | null>(null);

	async function test() {
		if (!channel || testing) return;
		testing = true;
		result = null;
		try {
			const report = await testChannel(channel.id);
			result = { ok: report.ok, message: report.message };
		} catch (cause) {
			result = { ok: false, message: cause instanceof Error ? cause.message : String(cause) };
		} finally {
			testing = false;
		}
	}

	const testDone = $derived(result?.ok === true);
	const minutes = $derived(expectedAlertMinutes(watched?.kind ?? ''));
</script>

{#snippet marker(done: boolean, index: number)}
	<span
		aria-hidden="true"
		class={`relative z-10 grid size-8 shrink-0 place-items-center rounded-full border font-mono text-sm transition-colors duration-300 ${done ? 'border-signal bg-signal text-on-signal' : 'border-line-strong bg-canvas text-ink-2'}`}
	>
		{#if done}<Check class="size-4" />{:else}{index}{/if}
	</span>
{/snippet}

<!-- A status line: icon and words, wrapping on a phone (a plate has a fixed height). -->
{#snippet status(tone: 'signal' | 'advisory' | 'warning' | 'ghost', text: string, pulse = false)}
	{@const Icon = tone === 'signal' ? CircleCheck : tone === 'advisory' ? CircleAlert : tone === 'warning' ? AlertTriangle : CircleDashed}
	<p class={`flex min-w-0 items-start gap-2 ${tone === 'signal' ? 'text-signal-ink' : tone === 'advisory' ? 'text-advisory-ink' : tone === 'warning' ? 'text-warning-ink' : 'text-ink-2'}`}>
		<Icon class={`mt-0.5 size-4 shrink-0 ${pulse ? 'animate-pulse' : ''} ${tone === 'signal' ? 'check-draw' : ''}`} aria-hidden="true" />
		<span class="min-w-0 [overflow-wrap:anywhere]">{text}</span>
	</p>
{/snippet}

{#if loading}
	<Skeleton class="h-72" />
{:else if loadError}
	<ErrorNotice error={loadError} />
{:else}
	<ol class="chain grid gap-8" aria-label={m.wizard_start_title()}>
		<!-- 1. What to watch -->
		<li class="link flex gap-4" data-done={watchDone}>
			{@render marker(watchDone, 1)}
			<div class="min-w-0 flex-1 pt-1">
				<h2 class="font-semibold text-ink">
					{m.wizard_start_watch_title()}
					<span class="sr-only">({watchDone ? m.wizard_start_step_done() : m.wizard_start_step_todo()})</span>
				</h2>
				<form class="mt-3 flex flex-wrap gap-2" onsubmit={watch} novalidate>
					<label for="quick-address" class="sr-only">{m.wizard_start_watch_label()}</label>
					<input
						id="quick-address"
						type="text"
						inputmode="url"
						class="input h-10 min-w-0 flex-1 basis-56"
						bind:value={address}
						placeholder={m.wizard_start_watch_placeholder()}
						autocomplete="off"
						autocapitalize="off"
						spellcheck="false"
						disabled={creating}
						aria-invalid={addressError ? 'true' : undefined}
						aria-describedby="quick-address-help"
						oninput={() => (addressError = null)}
					/>
					<Button type="submit" variant={watchDone ? 'secondary' : 'primary'} class="!h-10" loading={creating}>{m.wizard_start_watch_submit()}</Button>
				</form>
				<p id="quick-address-help" class={`mt-1.5 text-[0.8125rem] ${addressError ? 'text-warning-ink' : 'text-ink-2'}`}>
					{addressError ?? m.wizard_start_watch_help()}
				</p>
				{#if createError}<ErrorNotice error={createError} class="mt-3" />{/if}

				<div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 text-sm" aria-live="polite">
					{#if watched}
						{#if probe === 'probing'}
							{@render status('ghost', m.wizard_start_watch_probing({ name: watched.name }), true)}
						{:else if probe === 'slow'}
							{@render status('ghost', m.wizard_start_watch_slow({ name: watched.name }))}
						{:else if !watched.last_error}
							{@render status('signal', m.wizard_start_watch_up({ name: watched.name }))}
						{:else if watched.error_kind === 'config'}
							{@render status('advisory', m.wizard_start_watch_config({ name: watched.name, error: watched.last_error }))}
						{:else}
							{@render status('warning', m.wizard_start_watch_down({ name: watched.name, error: watched.last_error }))}
						{/if}
					{:else if existingTargets > 0}
						<span class="text-ink-2">{m.wizard_start_watch_existing({ count: existingTargets })}</span>
					{/if}
					<a class="inline-flex min-h-10 items-center gap-1 font-medium text-signal-ink hover:underline" href={withBase('/targets/new')} target="_blank" rel="noopener">
						{m.wizard_start_watch_other()}<ExternalLink class="size-3.5" aria-hidden="true" />
					</a>
				</div>
			</div>
		</li>

		<!-- 2. Where the alert goes -->
		<li class="link flex gap-4" data-done={channelDone}>
			{@render marker(channelDone, 2)}
			<div class="min-w-0 flex-1 pt-1">
				<h2 class="font-semibold text-ink">
					{m.wizard_start_where_title()}
					<span class="sr-only">({channelDone ? m.wizard_start_step_done() : m.wizard_start_step_todo()})</span>
				</h2>
				{#if channel && picking === null}
					<p class="mt-2 text-sm text-ink" aria-live="polite">{m.wizard_start_where_saved({ name: channel.name })}</p>
					{#if ntfyLink}
						<p class="mt-1 max-w-[60ch] text-[0.8125rem] text-ink-2">{m.wizard_start_ntfy_hint({ topic: String(channel.settings?.topic ?? '') })}</p>
						<a class="mt-1 inline-flex min-h-10 items-center gap-1 text-sm font-medium break-all text-signal-ink hover:underline" href={ntfyLink} target="_blank" rel="noopener">
							{ntfyLink}<ExternalLink class="size-3.5 shrink-0" aria-hidden="true" />
						</a>
					{/if}
				{:else if picking === null}
					<p class="mt-1 text-[0.8125rem] text-ink-2">{m.wizard_start_where_help()}</p>
				{/if}

				{#if picking === null}
					<div class="mt-3 flex flex-wrap gap-2">
						{#each quick as option (option.kind)}
							{@const Icon = kindIcon(option.kind)}
							<Button size="sm" variant="secondary" class="!h-10" onclick={() => pick(option.kind)}>
								<Icon class="size-4" aria-hidden="true" />
								{option.label}
							</Button>
						{/each}
						<Button size="sm" variant="ghost" class="!h-10" onclick={() => pick('')}>{m.wizard_start_where_other()}</Button>
					</div>
				{:else}
					{#if picking === 'ntfy'}
						<p class="mt-2 max-w-[60ch] text-[0.8125rem] text-ink-2">{m.wizard_start_ntfy_before()}</p>
					{/if}
					<div class="mt-3 min-w-0">
						{#key picking}
							<ChannelForm {kinds} initialKind={picking || undefined} initialSettings={presetSettings} onsaved={saved} oncancel={() => (picking = null)} />
						{/key}
					</div>
				{/if}
			</div>
		</li>

		<!-- 3. The proof -->
		<li class="link flex gap-4" data-done={testDone}>
			{@render marker(testDone, 3)}
			<div class="min-w-0 flex-1 pt-1">
				<h2 class="font-semibold text-ink">
					{m.wizard_start_test_title()}
					<span class="sr-only">({testDone ? m.wizard_start_step_done() : m.wizard_start_step_todo()})</span>
				</h2>
				<p class="mt-1 text-[0.8125rem] text-ink-2">
					{channel ? m.wizard_start_test_help({ name: channel.name }) : m.wizard_start_test_need_channel()}
				</p>
				<Button class="mt-3 !h-10" variant={channel && !testDone ? 'primary' : 'secondary'} disabled={!channel} loading={testing} onclick={test}>
					{m.wizard_start_test_send()}
				</Button>
				{#if result && channel}
					<div class="mt-3 text-sm" aria-live="polite">
						{#if result.ok}
							{@render status('signal', m.wizard_channel_test_ok({ name: channel.name }))}
						{:else}
							{@render status('warning', m.wizard_channel_test_failed({ error: result.message }))}
						{/if}
					</div>
				{/if}
				{#if testDone && channel}
					<p class="mt-3 max-w-[60ch] text-sm text-ink">
						{m.wizard_start_expect({ name: watched?.name ?? m.wizard_start_expect_any(), channel: channel.name, minutes })}
					</p>
				{/if}
			</div>
		</li>
	</ol>
{/if}

<style>
	/* The chain: a line runs down from each marker to the next, and turns to the
	   signal colour once the link above it is proven. */
	.link {
		position: relative;
	}
	.link:not(:last-child)::before {
		content: '';
		position: absolute;
		top: 2rem;
		bottom: -2rem;
		left: calc(1rem - 1px);
		width: 2px;
		background: var(--c-line);
		transition: background-color 300ms ease-out;
	}
	.link[data-done='true']:not(:last-child)::before {
		background: var(--c-signal);
	}
	@media (prefers-reduced-motion: reduce) {
		.link:not(:last-child)::before {
			transition: none;
		}
	}
</style>
