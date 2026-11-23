<script lang="ts">
	/**
	 * Dashboard B, "NOC": site, then category, then monitor. Where it breaks
	 * before what breaks: the sunburst and the site × category matrix, the
	 * problems grouped by site beside the event log, four top-5 lists, then
	 * backups by site, expiries, agents, services, the alert response, SLOs,
	 * maintenance and the status pages.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import KpiTiles from './widgets/KpiTiles.svelte';
	import FleetSunburst from './widgets/FleetSunburst.svelte';
	import SiteMatrix from './widgets/SiteMatrix.svelte';
	import ProblemsTable from './widgets/ProblemsTable.svelte';
	import RecentEvents from './widgets/RecentEvents.svelte';
	import TopConsumers from './widgets/TopConsumers.svelte';
	import BackupsTable from './widgets/BackupsTable.svelte';
	import ExpiriesCard from './widgets/ExpiriesCard.svelte';
	import AgentsCard from './widgets/AgentsCard.svelte';
	import ServicesTable from './widgets/ServicesTable.svelte';
	import AlertsResponse from './widgets/AlertsResponse.svelte';
	import SloCard from './widgets/SloCard.svelte';
	import MaintenanceCard from './widgets/MaintenanceCard.svelte';
	import StatusPagesCard from './widgets/StatusPagesCard.svelte';
</script>

<div class="flex min-w-0 flex-col gap-3.5">
	<KpiTiles />
	<div class="grid grid-flow-row-dense grid-cols-12 gap-3.5">
		<FleetSunburst span={4} />
		<SiteMatrix span={8} />
		<ProblemsTable span={8} bySite title={m.w9layout_noc_problems_title()} />
		<RecentEvents span={4} limit={7} />
		<TopConsumers span={3} metric="cpu" />
		<TopConsumers span={3} metric="mem" />
		<TopConsumers span={3} metric="disk" />
		<TopConsumers span={3} metric="net" />
		<BackupsTable span={6} bySite />
		<ExpiriesCard span={3} limit={6} />
		<AgentsCard span={3} />
		<ServicesTable span={6} limit={6} />
		<AlertsResponse span={3} />
		<SloCard span={3} />
		<MaintenanceCard span={8} />
		<StatusPagesCard span={4} />
	</div>
</div>
