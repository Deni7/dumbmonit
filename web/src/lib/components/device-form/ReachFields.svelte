<script lang="ts">
	/**
	 * "Reached through" (relay agents) and "Several locations" of the device form. Both are
	 * exclusive: probing from several locations replaces the relay. Rendered as grid items of
	 * the form's two-column grid.
	 */
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';
	import type { CollectorInfo } from '#lib/api/index.js';
	import type { RelayAgent } from '#lib/api/types.js';
	import { Field, Toggle } from '#lib/ui/index.js';

	interface Props {
		collector: CollectorInfo;
		canRelay: boolean;
		canMultisite: boolean;
		/** Relay agents the server knows (the edited monitor itself left out). */
		relays: RelayAgent[];
		/** Set once the list came back, so the "no relay yet" hint never flashes before it. */
		relaysRead: boolean;
		editing: boolean;
		viaAgent: number | null;
		msOn: boolean;
		msServer: boolean;
		msAgents: number[];
		msQuorum: number | null;
	}

	let {
		collector,
		canRelay,
		canMultisite,
		relays,
		relaysRead,
		editing,
		viaAgent = $bindable(),
		msOn = $bindable(),
		msServer = $bindable(),
		msAgents = $bindable(),
		msQuorum = $bindable()
	}: Props = $props();

	const relayCandidates = $derived(relays);
	const msTotal = $derived((msServer ? 1 : 0) + msAgents.length);
	const msTooFew = $derived(msOn && msTotal < 2);
	/** An unknown agent id (deleted since) stays listed so it can be unticked. */
	const msUnknownAgents = $derived(msAgents.filter((id) => !relays.some((r) => r.id === id)));
	const msNotReady = $derived(msOn && relays.some((r) => msAgents.includes(r.id) && !r.relay));
	function toggleMsAgent(id: number, on: boolean) {
		msAgents = on ? [...msAgents.filter((x) => x !== id), id] : msAgents.filter((x) => x !== id);
	}

	// The quorum cannot exceed the number of locations left.
	$effect(() => {
		if (msQuorum !== null && msQuorum > Math.max(msTotal, 1)) msQuorum = null;
	});

	/** The chosen relay, when it has not declared `relay: true`: probes would wait forever. */
	const relayNotReady = $derived(
		viaAgent !== null && relays.some((r) => r.id === viaAgent && !r.relay)
	);
	function relayLabel(relay: RelayAgent): string {
		return relay.site ? m.deviceform_form_relay_site({ name: relay.name, site: relay.site }) : relay.name;
	}
</script>

{#if canRelay && !(canMultisite && msOn) && (relayCandidates.length > 0 || viaAgent !== null)}
	<Field
		label={m.deviceform_form_relay_label()}
		for="target-relay"
		help={relayNotReady ? m.deviceform_form_relay_not_ready() : undefined}
	>
		<select id="target-relay" class="input" bind:value={viaAgent}>
			<option value={null}>{m.deviceform_form_relay_direct()}</option>
			{#each relayCandidates as relay (relay.id)}
				<option value={relay.id}>{relay.relay ? relayLabel(relay) : m.deviceform_form_relay_off({ label: relayLabel(relay) })}</option>
			{/each}
			{#if viaAgent !== null && !relayCandidates.some((r) => r.id === viaAgent)}
				<option value={viaAgent}>{m.deviceform_form_relay_agent_n({ id: viaAgent })}</option>
			{/if}
		</select>
	</Field>
{:else if canRelay && relaysRead && !editing}
	<!-- No relay yet: say once that a device on another network is reachable too. -->
	<p class="text-[0.8125rem] leading-relaxed text-ink-2 sm:col-span-2">
		<a href={withBase('/targets/new?kind=agent&via=relay')} class="font-semibold text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink">{m.deviceform_form_remote_link()}</a>
	</p>
{/if}

{#if canMultisite && (relayCandidates.length > 0 || msOn)}
	<div class="grid gap-3 sm:col-span-2">
		<Field
			label={m.deviceform_multisite_label()}
			for="target-multisite"
			inline
			help={viaAgent !== null && !msOn ? m.deviceform_multisite_exclusive() : undefined}
		>
			<Toggle
				id="target-multisite"
				bind:checked={msOn}
				disabled={viaAgent !== null && !msOn}
				label={m.deviceform_multisite_label()}
				onchange={(on) => {
					if (on) viaAgent = null;
				}}
			/>
		</Field>
		{#if msOn}
			<fieldset class="grid gap-1 rounded-[var(--radius-card)] border border-line p-3">
				<legend class="px-1 text-sm font-semibold text-ink">{m.deviceform_multisite_locations()}</legend>
				<label class="flex min-h-10 items-center gap-3 text-sm text-ink">
					<input type="checkbox" class="size-4 accent-[var(--color-signal)]" bind:checked={msServer} />
					{m.deviceform_multisite_server()}
				</label>
				{#each relayCandidates as relay (relay.id)}
					<label class="flex min-h-10 items-center gap-3 text-sm text-ink">
						<input
							type="checkbox"
							class="size-4 accent-[var(--color-signal)]"
							checked={msAgents.includes(relay.id)}
							onchange={(e) => toggleMsAgent(relay.id, e.currentTarget.checked)}
						/>
						<span class="min-w-0 break-words">{relay.relay ? relayLabel(relay) : m.deviceform_form_relay_off({ label: relayLabel(relay) })}</span>
					</label>
				{/each}
				{#each msUnknownAgents as id (id)}
					<label class="flex min-h-10 items-center gap-3 text-sm text-ink">
						<input type="checkbox" class="size-4 accent-[var(--color-signal)]" checked onchange={(e) => toggleMsAgent(id, e.currentTarget.checked)} />
						{m.deviceform_form_relay_agent_n({ id })}
					</label>
				{/each}
				{#if msTooFew}
					<p class="text-[0.8125rem] text-warning" role="alert">{m.deviceform_multisite_too_few()}</p>
				{:else if msNotReady}
					<p class="text-[0.8125rem] text-ink-2">{m.deviceform_multisite_not_ready()}</p>
				{/if}
			</fieldset>
			<Field label={m.deviceform_multisite_quorum_label()} for="target-quorum" help={m.deviceform_multisite_quorum_help()}>
				<select id="target-quorum" class="input" bind:value={msQuorum}>
					<option value={null}>{m.deviceform_multisite_quorum_majority()}</option>
					{#each Array.from({ length: Math.max(msTotal, 1) }, (_, i) => i + 1) as n (n)}
						<option value={n}>{m.deviceform_multisite_quorum_n({ n, total: msTotal })}</option>
					{/each}
				</select>
			</Field>
		{/if}
	</div>
{/if}
