<script lang="ts">
	/**
	 * Before the install command: can machines reach the encrypted agent
	 * channel where the command will send them?
	 *
	 * - Straight to this server: the server dials its own channel at that
	 *   address (`POST /api/agent/tls/check`) and says what answered. An
	 *   unpublished Docker port is the first cause of "the agent could not
	 *   reach the server", so the fix is spelled out with the real ports.
	 * - Through the web address (`transport: websocket`, behind Cloudflare
	 *   Tunnel or a reverse proxy): the server opens the agent WebSocket there
	 *   and completes the channel handshake inside it; no port to publish, the
	 *   proxy must forward WebSockets.
	 * - Through a relay: the server cannot see the site's network. The relay
	 *   opens its gateway as soon as it is told (within seconds), then tests it
	 *   itself at the site's address, as a machine of the site would, and
	 *   reports the outcome. The command shows once that test passes; a failed
	 *   test is explained instead. An older relay only says it listens.
	 *   What the server knows blocks the gateway (`problem`: switched off,
	 *   relay agent too old or offline, port it cannot open, loopback only, no
	 *   encrypted channel) is said with its fix rather than waited on.
	 *
	 * The command (`children`) shows once the channel answers, or when the
	 * administrator chooses to see it anyway (a check made from the server can
	 * fail behind NAT without hairpin although agents outside connect fine).
	 *
	 * Transport (`transport`, `onTransport`): when the agent port does not
	 * answer, the administrator can send agents through the web address
	 * instead (WebSocket), or back to the agent port; the parent issues the
	 * command again with that choice and the check runs again.
	 */
	import type { Snippet } from 'svelte';
	import { untrack } from 'svelte';
	import { Lock, RadioTower, AlertTriangle } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { formatDateTime, formatRelative } from '#lib/format.js';
	import { checkAgentChannel } from '#lib/api/agent_tls.js';
	import type { AgentChannelCheck, AgentTransportWish } from '#lib/api/types.js';
	import { Button, CopyBlock, ErrorNotice, Led } from '#lib/ui/index.js';
	import { channelReady, firewallCommands, gatewayListenLine, portOwnerCommands, relayStateOf } from './agent-channel.js';

	interface Props {
		/** Address machines reach the web interface at (the browser's origin). */
		baseUrl: string;
		relayId?: number | null;
		relayHost?: string | null;
		/** Transport the command uses: `auto` lets the server decide. */
		transport?: AgentTransportWish;
		/** Asks the parent for a command with another transport. */
		onTransport?: (next: AgentTransportWish) => void;
		children: Snippet;
	}
	let { baseUrl, relayId = null, relayHost = null, transport = 'auto', onTransport, children }: Props = $props();

	let result = $state<AgentChannelCheck | null>(null);
	let checking = $state(false);
	let error = $state<unknown>(null);
	let showAnyway = $state(false);
	let timer: ReturnType<typeof setTimeout> | null = null;

	async function run(): Promise<boolean> {
		if (timer) clearTimeout(timer);
		timer = null;
		checking = true;
		error = null;
		try {
			result = await checkAgentChannel({
				base_url: baseUrl,
				transport,
				...(relayId !== null ? { relay_id: relayId, relay_host: relayHost } : {})
			});
			// A relay opens its gateway, then tests it: look again until it passes.
			if (result.relay && relayStateOf(result.relay) !== 'ready') timer = setTimeout(run, 5000);
			return true;
		} catch (cause) {
			error = cause;
			return false;
		} finally {
			checking = false;
		}
	}

	$effect(() => {
		// Checked again when the command changes transport.
		void transport;
		untrack(run);
		return () => {
			if (timer) clearTimeout(timer);
		};
	});

	const relayState = $derived(result?.relay ? relayStateOf(result.relay) : null);
	const relayAddress = $derived(result?.relay ? `${result.relay.host}:${result.relay.port}` : '');

	const address = $derived(result?.host && result.port ? `${result.host}:${result.port}` : '');
	const port = $derived(String(result?.relay?.port ?? result?.port ?? 8443));
	const mapping = $derived(`${result?.port ?? 8443}:${result?.listen_port ?? 8443}`);
	const ready = $derived(showAnyway || error !== null || channelReady(result));
	const firewall = $derived(firewallCommands(port));
	/** The relay's own test, labelled as such: who ran it, where, and when. */
	const testedBy = $derived.by(() => {
		const relay = result?.relay;
		if (!relay?.checked_at_ms) return null;
		const at = new Date(relay.checked_at_ms);
		return { text: m.deviceform_channelcheck_gateway_tested_by({ relay: relay.relay_name, address: relayAddress, when: formatRelative(at) }), title: formatDateTime(at) };
	});
	/** What blocks the relay's gateway, with its fix: a command to copy, and the relay's page where the setting lives. */
	const blocked = $derived.by(() => {
		const relay = result?.relay;
		if (!relay?.problem) return null;
		const name = relay.relay_name;
		switch (relay.problem) {
			case 'closed':
				return {
					title: m.deviceform_channelcheck_gateway_closed_title({ relay: name }),
					body: m.deviceform_channelcheck_gateway_closed_body({ relay: name, setting: m.devicesb_relay_panel_gateway() }),
					copy: null,
					copyLabel: ''
				};
			case 'too_old':
				return {
					title: m.deviceform_channelcheck_gateway_too_old_title({ relay: name, version: relay.agent_version ?? '?' }),
					body: m.deviceform_channelcheck_gateway_too_old_body({ relay: name, min: relay.min_agent_version ?? '0.14.2' }),
					copy: null,
					copyLabel: ''
				};
			case 'offline':
				return {
					title: m.deviceform_channelcheck_gateway_offline_title({ relay: name }),
					body: m.deviceform_channelcheck_gateway_offline_body({ relay: name }),
					copy: null,
					copyLabel: ''
				};
			case 'bind_failed':
				return {
					title: m.deviceform_channelcheck_gateway_bind_title({ relay: name, port }),
					body: m.deviceform_channelcheck_gateway_bind_body({ relay: name, error: relay.bind_error ?? '', port }),
					copy: portOwnerCommands(port),
					copyLabel: m.deviceform_channelcheck_gateway_copy_owner()
				};
			case 'loopback':
				return {
					title: m.deviceform_channelcheck_gateway_loopback_title({ relay: name, address: relay.listen ?? '' }),
					body: m.deviceform_channelcheck_gateway_loopback_body({ relay: name }),
					copy: gatewayListenLine(port),
					copyLabel: m.deviceform_channelcheck_gateway_copy_listen()
				};
			case 'no_tunnel':
				return {
					title: m.deviceform_channelcheck_gateway_no_tunnel_title({ relay: name }),
					body: m.deviceform_channelcheck_gateway_no_tunnel_body({ relay: name }),
					copy: null,
					copyLabel: ''
				};
			default:
				return null;
		}
	});
	const problem = $derived.by(() => {
		const outcome = result?.outcome;
		if (!result || result.relay || !outcome || outcome === 'ok') return null;
		const p = String(result.port ?? 8443);
		const host = result.host ?? '';
		if (result.transport === 'websocket') {
			return { title: m.deviceform_channelcheck_websocket_title({ address }), body: m.deviceform_channelcheck_websocket_body(), compose: false };
		}
		switch (outcome) {
			case 'refused':
				return { title: m.deviceform_channelcheck_refused_title({ port: p }), body: m.deviceform_channelcheck_refused_body({ address }), compose: true };
			case 'timeout':
				return { title: m.deviceform_channelcheck_timeout_title({ port: p }), body: m.deviceform_channelcheck_timeout_body({ address }), compose: true };
			case 'dns':
				return { title: m.deviceform_channelcheck_dns_title({ host }), body: m.deviceform_channelcheck_dns_body({ host }), compose: false };
			default:
				return { title: m.deviceform_channelcheck_tls_title({ port: p }), body: m.deviceform_channelcheck_tls_body({ address, port: p }), compose: false };
		}
	});
	/** The other way agents can reach the channel, when this one fails: offered only straight to this server. */
	const otherTransport = $derived.by((): AgentTransportWish | null => {
		if (!onTransport || !result || result.relay || !problem) return null;
		if (result.transport === 'websocket') return 'direct';
		return result.websocket && result.outcome !== 'dns' ? 'websocket' : null;
	});
</script>

<div class="grid gap-3 empty:hidden" aria-live="polite">
	{#if checking && !result}
		<p class="flex items-center gap-2 text-sm text-ink-2">
			<Led tone="ghost" size="sm" blink />
			<span class="min-w-0 break-words">{m.deviceform_channelcheck_checking()}</span>
		</p>
	{:else if error}
		<ErrorNotice {error} title={m.deviceform_channelcheck_error()} />
	{:else if result && !result.enabled}
		<p class="text-sm text-ink-2">{m.deviceform_channelcheck_disabled()}</p>
	{:else if result?.relay}
		{#if relayState === 'blocked' && blocked}
			<div role="alert" class="grid gap-2 rounded-[var(--radius-card)] border border-warning/50 bg-warning-soft px-3 py-3">
				<p class="flex items-start gap-2 text-sm font-semibold text-warning-ink">
					<AlertTriangle class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
					<span class="min-w-0 break-words">{blocked.title}</span>
				</p>
				<p class="max-w-prose text-sm break-words text-ink">{blocked.body}</p>
				{#if blocked.copy}
					<CopyBlock value={blocked.copy} label={blocked.copyLabel} />
				{/if}
				<div class="flex flex-wrap items-center gap-2">
					<Button variant="secondary" size="sm" loading={checking} onclick={run}>{m.deviceform_channelcheck_retry()}</Button>
					<a class="inline-flex min-h-10 items-center text-sm font-semibold text-ink underline underline-offset-2" href={withBase(`/targets/${result.relay.relay_id}`)}>
						{m.deviceform_channelcheck_gateway_open_relay({ relay: result.relay.relay_name })}
					</a>
				</div>
			</div>
		{:else if relayState === 'ready'}
			<p class="flex items-start gap-2 text-sm text-ink">
				<RadioTower class="mt-0.5 size-4 shrink-0 text-signal-ink" aria-hidden="true" />
				<span class="min-w-0 break-words">{m.deviceform_channelcheck_relay_ready({ relay: result.relay.relay_name, port })}</span>
			</p>
			{#if testedBy}
				<p class="-mt-1 max-w-prose text-[0.8125rem] text-ink-2" title={testedBy.title}>{testedBy.text}</p>
			{/if}
		{:else if relayState === 'unreachable'}
			<div role="alert" class="grid gap-2 rounded-[var(--radius-card)] border border-warning/50 bg-warning-soft px-3 py-3">
				<p class="flex items-start gap-2 text-sm font-semibold text-warning-ink">
					<AlertTriangle class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
					<span class="min-w-0 break-words">{m.deviceform_channelcheck_relay_unreachable_title({ relay: result.relay.relay_name, address: relayAddress })}</span>
				</p>
				<p class="max-w-prose text-sm break-words text-ink">
					{m.deviceform_channelcheck_relay_unreachable_body({ error: result.relay.check_error ?? '', port })}
				</p>
				{#if testedBy}
					<p class="max-w-prose text-[0.8125rem] text-ink-2" title={testedBy.title}>{testedBy.text}</p>
				{/if}
			</div>
		{:else if relayState === 'testing'}
			<p class="flex items-start gap-2 text-sm text-ink">
				<Led tone="ghost" size="sm" blink />
				<span class="min-w-0 break-words">{m.deviceform_channelcheck_relay_testing({ relay: result.relay.relay_name, address: relayAddress })}</span>
			</p>
		{:else}
			<p class="flex items-start gap-2 text-sm text-ink">
				<Led tone="ghost" size="sm" blink />
				<span class="min-w-0 break-words">{m.deviceform_channelcheck_relay_waiting({ relay: result.relay.relay_name, port })}</span>
			</p>
		{/if}
		{#if relayState !== 'blocked'}
			<div class="grid gap-1.5">
				<p class="text-sm text-ink-2">{m.deviceform_channelcheck_relay_firewall({ relay: result.relay.relay_name, port })}</p>
				<CopyBlock value={firewall} label={m.deviceform_channelcheck_copy_firewall()} />
			</div>
		{/if}
	{:else if problem}
		<div role="alert" class="grid gap-3 rounded-[var(--radius-card)] border border-warning/50 bg-warning-soft px-3 py-3">
			<p class="flex items-start gap-2 text-sm font-semibold text-warning-ink">
				<AlertTriangle class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
				<span class="min-w-0 break-words">{problem.title}</span>
			</p>
			<p class="max-w-prose text-sm break-words text-ink">{problem.body}</p>
			{#if problem.compose}
				<p class="max-w-prose text-sm text-ink">{m.deviceform_channelcheck_compose({ mapping })}</p>
				<CopyBlock value={`    ports:\n      - "${mapping}"`} label={m.deviceform_channelcheck_copy_compose()} />
				<p class="max-w-prose text-sm text-ink">{m.deviceform_channelcheck_firewall({ port })}</p>
				<CopyBlock value={firewall} label={m.deviceform_channelcheck_copy_firewall()} />
			{/if}
			<p class="max-w-prose text-[0.8125rem] text-ink-2">{m.deviceform_channelcheck_hairpin()}</p>
			{#if otherTransport === 'websocket'}
				<p class="max-w-prose text-sm text-ink">{m.deviceform_channelcheck_websocket_hint()}</p>
			{/if}
			<div class="flex flex-wrap items-center gap-2">
				<Button variant="secondary" size="sm" loading={checking} onclick={run}>{m.deviceform_channelcheck_retry()}</Button>
				{#if otherTransport && onTransport}
					{@const next = otherTransport}
					<Button variant="secondary" size="sm" disabled={checking} onclick={() => onTransport?.(next)}>
						{next === 'websocket' ? m.deviceform_channelcheck_use_websocket() : m.deviceform_channelcheck_use_direct()}
					</Button>
				{/if}
				{#if !showAnyway}
					<Button variant="ghost" size="sm" onclick={() => (showAnyway = true)}>{m.deviceform_channelcheck_show_anyway()}</Button>
				{/if}
			</div>
		</div>
	{:else if result?.outcome === 'ok'}
		<p class="flex items-center gap-2 text-sm text-ink">
			<Lock class="size-4 shrink-0 text-signal-ink" aria-hidden="true" />
			<span class="min-w-0 break-words">{result?.transport === 'websocket' ? m.deviceform_channelcheck_websocket_ok({ address }) : m.deviceform_channelcheck_ok({ address })}</span>
		</p>
	{/if}
	{#if !ready && result?.relay}
		<div>
			<Button variant="ghost" size="sm" onclick={() => (showAnyway = true)}>{m.deviceform_channelcheck_show_anyway()}</Button>
		</div>
	{/if}
</div>

{#if ready}
	{@render children()}
{/if}
