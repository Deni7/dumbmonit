<script lang="ts">
	/** Under a monitor's header: probe outcome, action errors, managed and agent-removal notices, setup hint. */
	import { ExternalLink } from 'lucide-svelte';
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';
	import type { CollectorInfo, ProbeReport, Target } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { ErrorNotice, Plate } from '#lib/ui/index.js';
	import ManagedNotice from '#lib/components/devices/ManagedNotice.svelte';
	import AgentRemoveNotice from '#lib/components/devices/agent/AgentRemoveNotice.svelte';

	interface Props {
		target: Target;
		collector: CollectorInfo | null;
		kindLabel: string;
		probeResult: ProbeReport | null;
		probeError: unknown;
		enabledError: unknown;
		deleteError: unknown;
		deleting: boolean;
		askAgentRemoval: boolean;
		probe: () => void | Promise<void>;
		remove: () => void | Promise<void>;
		loadTarget: () => void | Promise<unknown>;
	}

	let {
		target,
		collector,
		kindLabel,
		probeResult,
		probeError,
		enabledError,
		deleteError,
		deleting,
		askAgentRemoval = $bindable(),
		probe,
		remove,
		loadTarget
	}: Props = $props();
</script>

<div aria-live="polite" class="empty:hidden">
	{#if probeResult}
		<div class="mt-4 flex flex-wrap items-center gap-2">
			<Plate tone="signal" label={m.devices_target_probe_done({ samples: probeResult.sample_count, series: probeResult.series.length })} size="md" />
			{#if probeResult.sample_count === 0}
				<span class="text-sm text-ink-2">{m.devices_target_probe_empty()}</span>
			{/if}
		</div>
	{/if}
</div>
{#if probeError}
	<ErrorNotice error={probeError} title={m.devices_target_err_probe()} onretry={probe} class="mt-4" />
{/if}
{#if enabledError}
	<ErrorNotice error={enabledError} title={m.devices_target_err_enabled()} class="mt-4" />
{/if}
{#if target?.managed}
	<ManagedNotice {target} canRelease={auth.can('monitors.manage')} onreleased={() => void loadTarget()} class="mt-4" />
{/if}
{#if askAgentRemoval && target}
	<AgentRemoveNotice {target} {deleting} ondelete={remove} oncancel={() => (askAgentRemoval = false)} />
{/if}
{#if deleteError}
	<ErrorNotice error={deleteError} title={m.devices_target_err_delete()} class="mt-4" />
{/if}
{#if collector?.setup.doc_url && target.last_error}
	<p class="mt-3 text-sm text-ink-2">
		{m.devices_target_check_reachable()}
		<a href={withBase(collector.setup.doc_url)} class="inline-flex min-h-10 items-center gap-1 text-ink hover:underline sm:min-h-0" target="_blank" rel="noreferrer">
			{m.devices_target_setup_notes({ kind: kindLabel })}
			<ExternalLink class="size-3.5" aria-hidden="true" />
		</a>
	</p>
{/if}
