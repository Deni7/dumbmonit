<script lang="ts">
	/** The main action of a monitor's header (probe now / enable) and its menu, or the read-only plate. */
	import { Activity } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Target } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, Plate } from '#lib/ui/index.js';
	import TargetMenu from '#lib/components/devices/TargetMenu.svelte';
	import SilenceControl from '#lib/components/devices/SilenceControl.svelte';

	interface Props {
		target: Target;
		refreshKey: number;
		probing: boolean;
		savingEnabled: boolean;
		deleting: boolean;
		probe: () => void | Promise<void>;
		setEnabled: (value: boolean) => void | Promise<void>;
		remove: () => void | Promise<void>;
		askRemoval: () => void;
	}

	let { target, refreshKey, probing, savingEnabled, deleting, probe, setEnabled, remove, askRemoval }: Props = $props();
</script>

<!-- One main action, then the gear: enable, snooze, logs, delete and Settings sit in its menu. -->
<div class="flex min-w-0 max-w-full flex-wrap items-center gap-2 lg:shrink-0 lg:justify-end">
	{#if target.enabled}
		<Button variant="primary" onclick={probe} loading={probing}>
			<Activity class="size-4" aria-hidden="true" />
			{m.devices_target_probe_now()}
		</Button>
	{:else if auth.can('monitors.manage')}
		<Button variant="primary" onclick={() => void setEnabled(true)} loading={savingEnabled}>
			{m.devices_target_enable()}
		</Button>
	{/if}
	{#if auth.can('monitors.manage')}
		<TargetMenu
			{target}
			{refreshKey}
			{savingEnabled}
			{deleting}
			onsetenabled={(value) => void setEnabled(value)}
			ondelete={remove}
			onasktoremove={askRemoval}
		/>
	{:else}
		{#if auth.canSilence && target.enabled}
			<SilenceControl {target} {refreshKey} />
		{/if}
		<Plate tone="ghost" label={auth.readOnlyLabel} size="md" />
	{/if}
</div>
