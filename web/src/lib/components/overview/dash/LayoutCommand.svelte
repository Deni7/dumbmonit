<script lang="ts">
	/**
	 * Dashboard A, "Command": a console of widgets on one wide screen, the most
	 * urgent top left. The three figures, then problems and the alert response,
	 * the fleet map, top consumers, SLOs, backups, expiries, services,
	 * maintenance, agents, the event log and the status pages.
	 */
	import KpiTiles from './widgets/KpiTiles.svelte';
	import ProblemsTable from './widgets/ProblemsTable.svelte';
	import AlertsResponse from './widgets/AlertsResponse.svelte';
	import FleetHoneycomb from './widgets/FleetHoneycomb.svelte';
	import TopConsumers from './widgets/TopConsumers.svelte';
	import SloCard from './widgets/SloCard.svelte';
	import BackupsTable from './widgets/BackupsTable.svelte';
	import ExpiriesCard from './widgets/ExpiriesCard.svelte';
	import ServicesTable from './widgets/ServicesTable.svelte';
	import MaintenanceCard from './widgets/MaintenanceCard.svelte';
	import AgentsCard from './widgets/AgentsCard.svelte';
	import RecentEvents from './widgets/RecentEvents.svelte';
	import StatusPagesCard from './widgets/StatusPagesCard.svelte';
</script>

<div class="flex min-w-0 flex-col gap-3.5">
	<KpiTiles />
	<div class="grid grid-flow-row-dense grid-cols-12 gap-3.5">
		<ProblemsTable span={8} />
		<AlertsResponse span={4} />
		<FleetHoneycomb span={5} />
		<TopConsumers span={4} />
		<SloCard span={3} />
		<BackupsTable span={7} />
		<ExpiriesCard span={5} limit={7} />
		<ServicesTable span={5} />
		<MaintenanceCard span={4} />
		<AgentsCard span={3} />
		<RecentEvents span={8} />
		<StatusPagesCard span={4} />
	</div>
</div>
