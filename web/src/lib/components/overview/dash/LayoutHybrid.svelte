<script lang="ts">
	/**
	 * Dashboard C, "Hybrid": act first, then read. A dark band on top, the
	 * same in both themes, holds the three figures, what to deal with now and
	 * the next 14 days; under it a rich grid to understand: one card per site,
	 * top consumers, backups, services, the alert response, SLOs, agents, the
	 * event log, the status pages and the maintenance calendar.
	 *
	 * The band wears the console's night tokens (`.dash-band` in app.css), so
	 * every widget inside it follows without a variant of its own.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import KpiTiles from './widgets/KpiTiles.svelte';
	import ProblemsTable from './widgets/ProblemsTable.svelte';
	import NextDays from './widgets/NextDays.svelte';
	import SiteCards from './widgets/SiteCards.svelte';
	import TopConsumers from './widgets/TopConsumers.svelte';
	import BackupsTable from './widgets/BackupsTable.svelte';
	import ServicesTable from './widgets/ServicesTable.svelte';
	import AlertsResponse from './widgets/AlertsResponse.svelte';
	import SloCard from './widgets/SloCard.svelte';
	import AgentsCard from './widgets/AgentsCard.svelte';
	import RecentEvents from './widgets/RecentEvents.svelte';
	import StatusPagesCard from './widgets/StatusPagesCard.svelte';
	import MaintenanceCard from './widgets/MaintenanceCard.svelte';
</script>

<div class="flex min-w-0 flex-col gap-3.5">
	<section class="dash-band flex flex-col gap-3.5 rounded-[var(--radius-card)] border border-line bg-canvas-deep p-3 text-ink sm:p-3.5" aria-label={m.w9layout_hybrid_band_label()}>
		<KpiTiles band />
		<div class="grid grid-flow-row-dense grid-cols-12 gap-3.5">
			<ProblemsTable span={8} title={m.w9layout_hybrid_act_now()} />
			<NextDays span={4} />
		</div>
	</section>
	<div class="grid grid-flow-row-dense grid-cols-12 gap-3.5">
		<SiteCards span={4} />
		<TopConsumers span={4} />
		<BackupsTable span={8} />
		<ServicesTable span={6} />
		<AlertsResponse span={3} />
		<SloCard span={3} />
		<AgentsCard span={4} />
		<RecentEvents span={5} />
		<StatusPagesCard span={3} />
		<MaintenanceCard span={12} />
	</div>
</div>
