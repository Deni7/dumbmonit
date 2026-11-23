<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The agents: how many, which versions they run (one bar split by version,
	 * the latest first, each part named), a way to update the ones behind, and
	 * whether the relays of the remote sites are connected. From the
	 * dashboards' store.
	 */
	import { formatRelative } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { versionShares, type VersionShare } from '../parts/format.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 3 }: { span?: Span } = $props();

	/** Relays listed; a larger fleet of sites is on Settings › Monitoring. */
	const RELAY_LIMIT = 4;

	const agents = $derived(widgetsStore.data?.agents ?? null);
	const shares = $derived(versionShares(agents?.versions ?? []));
	const behind = $derived(shares.filter((s) => s.rank !== 'latest').reduce((sum, s) => sum + s.count, 0));
	const relays = $derived((agents?.relays ?? []).slice(0, RELAY_LIMIT));

	const FILL: Record<VersionShare['rank'], string> = {
		latest: 'var(--color-river-up)',
		behind: 'var(--color-advisory)',
		old: 'var(--color-warning)'
	};
</script>

{#snippet manage()}
	<a href={withBase('/settings/monitoring#agents')}>{m.w9dash_agents_manage()}</a>
{/snippet}

<!-- Settings › Monitoring is for administrators: the others get no footer link. -->
<WidgetCard title={m.w9dash_agents_title()} {span} foot={auth.isAdmin ? manage : undefined}>
	{#if !agents && widgetsStore.loading}
		<Skeleton class="h-6 w-full" rows={3} />
	{:else if !agents}
		<ErrorNotice error={widgetsStore.error} title={m.w9dash_common_error()} compact onretry={() => void widgetsStore.refresh()} />
	{:else if agents.total === 0 && relays.length === 0}
		<p class="m-0 py-2 text-sm text-ink-2">{m.w9dash_agents_empty()}</p>
		{#if auth.isAdmin}
			<Button size="sm" variant="secondary" href="/targets/new?kind=agent">{m.w9dash_agents_install()}</Button>
		{/if}
	{:else}
		<div class="flex flex-wrap items-center justify-between gap-2">
			<span class="text-base font-semibold text-ink">{m.w9dash_agents_total({ count: agents.total })}</span>
			{#if behind > 0 && auth.isAdmin}
				<Button size="sm" variant="secondary" href="/settings/monitoring#agents">{m.w9dash_agents_update({ count: behind })}</Button>
			{/if}
		</div>
		{#if shares.length > 0}
			<div class="mt-2 mb-1.5 flex h-2.5 gap-0.5 overflow-hidden rounded-full" aria-hidden="true">
				{#each shares as share (share.version)}
					<span class="block h-full" style={`flex:${share.count} 1 0;background:${FILL[share.rank]}`}></span>
				{/each}
			</div>
			<ul class="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-xs text-ink-2">
				{#each shares as share (share.version)}
					<li class="inline-flex items-center gap-1.5">
						<span aria-hidden="true" class="size-2 shrink-0 rounded-[2px]" style={`background:${FILL[share.rank]}`}></span>
						{share.rank === 'latest'
							? m.w9dash_agents_on_latest({ count: share.count, version: share.version })
							: m.w9dash_agents_on_version({ count: share.count, version: share.version })}
					</li>
				{/each}
			</ul>
		{/if}
		{#if relays.length > 0}
			<ul class="m-0 mt-2.5 list-none p-0">
				{#each relays as relay (relay.name)}
					<li class="flex min-w-0 items-center gap-2.5 border-t border-line py-2">
						<Plate tone={relay.connected ? 'signal' : 'warning'} label={relay.connected ? m.w9dash_agents_connected() : m.w9dash_agents_disconnected()} class="shrink-0" />
						<div class="min-w-0 flex-1">
							<b class="block truncate text-sm font-semibold text-ink">{relay.name}</b>
							<span class="block truncate text-[0.8125rem] text-ink-2">
								{relay.last_seen ? m.w9dash_agents_last_contact({ ago: formatRelative(relay.last_seen) }) : m.w9dash_agents_never_seen()}
							</span>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</WidgetCard>
