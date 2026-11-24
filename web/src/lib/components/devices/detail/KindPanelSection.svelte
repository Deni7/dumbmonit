<script lang="ts">
	/** What a kind of device has to show beyond charts (guests, backup calendar, disks…), loaded once per kind. */
	import { m } from '#lib/paraglide/messages.js';
	import type { Target } from '#lib/api/index.js';
	import { loadKindPanel, hasKindPanel, type KindPanel } from '#lib/components/devices/kinds.js';
	import { Skeleton } from '#lib/ui/index.js';

	let { target }: { target: Target } = $props();

	// Dynamically imported (`kinds.ts`): loaded once per kind, not on every poll.
	let kindPanelComponent = $state<KindPanel | null>(null);
	let kindPanelLoadedFor = $state<string | null>(null);
	$effect(() => {
		const kind = target.kind;
		if (!kind || !hasKindPanel(kind)) {
			kindPanelComponent = null;
			kindPanelLoadedFor = null;
			return;
		}
		if (kindPanelLoadedFor === kind) return;
		let cancelled = false;
		loadKindPanel(kind)?.then((component) => {
			if (cancelled) return;
			kindPanelComponent = component;
			kindPanelLoadedFor = kind;
		});
		return () => {
			cancelled = true;
		};
	});
</script>

<section class="mt-6 flex flex-col gap-6" aria-label={m.devices_target_details()}>
	{#if kindPanelComponent && kindPanelLoadedFor === target.kind}
		{@const Panel = kindPanelComponent}
		<Panel {target} />
	{:else}
		<Skeleton class="h-40 w-full rounded-[var(--radius-card)]" />
	{/if}
</section>
