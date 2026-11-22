<script lang="ts">
	import { docTitle } from '#lib/stores/page-title.svelte.js';
	/**
	 * Status: what you publish. Pages (the public status pages and the
	 * announcements shown on them), Reports (scheduled reports, deadlines),
	 * SLA (the availability report of a period, administrators) and SLOs
	 * (availability targets and their error budget). `/reports` and
	 * `/sla` forward to their tab; the page editor stays at `/status/<id>`.
	 */
	import { FileCheck, FileText, Gauge, Globe } from 'lucide-svelte';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { m } from '#lib/paraglide/messages.js';
	import { PageHeader, Tabs, type TabItem } from '#lib/ui/index.js';
	import StatusPagesSection from '#lib/components/status/StatusPagesSection.svelte';
	import ReportsTab from '#lib/components/reports/ReportsTab.svelte';
	import SlaTab from '#lib/components/reports/SlaTab.svelte';
	import SloTab from '#lib/components/slo/SloTab.svelte';

	const tabs: TabItem[] = $derived([
		{ id: 'pages', label: m.status_tab_pages(), icon: Globe },
		{ id: 'reports', label: m.reports_page_title(), icon: FileText },
		{ id: 'sla', label: m.shell_nav_sla(), icon: FileCheck, hidden: !(auth.isAdmin || (auth.mspOn && (auth.msp?.clients.length ?? 0) > 0)) },
		// Team-restricted accounts: the server keeps SLOs to unrestricted accounts.
		{ id: 'slo', label: m.status_tab_slos(), icon: Gauge, hidden: auth.scoped }
	]);
</script>

<svelte:head><title>{docTitle(m.status_heading())}</title></svelte:head>

<PageHeader title={m.status_heading()} />

<Tabs {tabs} label={m.status_tabs_label()}>
	{#snippet panel(id)}
		{#if id === 'reports'}
			<ReportsTab />
		{:else if id === 'sla'}
			<SlaTab />
		{:else if id === 'slo'}
			<SloTab />
		{:else}
			<StatusPagesSection />
		{/if}
	{/snippet}
</Tabs>
