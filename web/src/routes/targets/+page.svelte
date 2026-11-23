<script lang="ts">
	/**
	 * Monitors: the estate seen four ways, as tabs of one destination. List
	 * (folders, order, filters, bulk actions), Logs (administrators),
	 * Security (threat map and CVEs) and Map (topology and world), last. The old
	 * `/map`, `/logs` and `/security` pages forward here.
	 *
	 * "Add a monitor", the one primary of the list, sits in the header as the
	 * Alerts page keeps its own action there. Only on the List tab (Logs has
	 * its own primary, Search) and not while the list is known to be empty,
	 * where the empty state offers it instead; the header keeps the button's
	 * height either way, so switching tabs moves nothing.
	 */
	import { page } from '$app/state';
	import { List, Network, Plus, ScrollText, ShieldAlert } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { Button, ClickSpark, PageHeader, Tabs, type TabItem } from '#lib/ui/index.js';
	import MonitorList from '#lib/components/monitors/MonitorList.svelte';
	import MapTab from '#lib/components/monitors/MapTab.svelte';
	import LogsTab from '#lib/components/monitors/LogsTab.svelte';
	import SecurityTab from '#lib/components/monitors/SecurityTab.svelte';

	const tabs: TabItem[] = $derived([
		{ id: 'list', label: m.topology_view_list(), icon: List },
		{ id: 'logs', label: m.logsearch_title(), icon: ScrollText, hidden: !auth.isAdmin },
		{ id: 'security', label: m.security_page_title(), icon: ShieldAlert },
		{ id: 'map', label: m.topology_title(), icon: Network }
	]);

	/** The List tab is open (it is the first tab, so also without `?tab=`). */
	const onList = $derived.by(() => {
		const asked = page.url.searchParams.get('tab');
		return !tabs.some((tab) => !tab.hidden && tab.id !== 'list' && tab.id === asked);
	});
	/** The list loaded and holds nothing: its empty state carries the button. */
	const knownEmpty = $derived(!alertsStore.loading && alertsStore.available && alertsStore.targets.length === 0);
</script>

<svelte:head><title>{m.devices_targets_page_title()}</title></svelte:head>

<PageHeader title={m.devices_targets_title()} class="min-h-10">
	{#snippet actions()}
		{#if auth.can('monitors.manage') && onList && !knownEmpty}
			<ClickSpark>
				<Button variant="primary" href="/targets/new">
					<Plus class="size-4" aria-hidden="true" />
					{m.devices_targets_add()}
				</Button>
			</ClickSpark>
		{/if}
	{/snippet}
</PageHeader>

<Tabs {tabs} label={m.monitors_tabs_label()}>
	{#snippet panel(id)}
		{#if id === 'map'}
			<MapTab />
		{:else if id === 'logs'}
			<LogsTab />
		{:else if id === 'security'}
			<SecurityTab />
		{:else}
			<MonitorList />
		{/if}
	{/snippet}
</Tabs>
