<script lang="ts">
	/**
	 * Status: what you publish. Pages (the public status pages and the
	 * announcements shown on them), Reports (scheduled reports, deadlines) and
	 * SLA (the availability report of a period, administrators). `/reports` and
	 * `/sla` forward to their tab; the page editor stays at `/status/<id>`.
	 */
	import { FileCheck, FileText, Globe } from 'lucide-svelte';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { m } from '#lib/paraglide/messages.js';
	import { PageHeader, Tabs, type TabItem } from '#lib/ui/index.js';
	import StatusPagesSection from '#lib/components/status/StatusPagesSection.svelte';
	import ReportsTab from '#lib/components/reports/ReportsTab.svelte';
	import SlaTab from '#lib/components/reports/SlaTab.svelte';

	const tabs: TabItem[] = $derived([
		{ id: 'pages', label: m.status_tab_pages(), icon: Globe },
		{ id: 'reports', label: m.reports_page_title(), icon: FileText },
		{ id: 'sla', label: m.shell_nav_sla(), icon: FileCheck, hidden: !(auth.isAdmin || (auth.mspOn && (auth.msp?.clients.length ?? 0) > 0)) }
	]);
</script>

<svelte:head><title>{m.status_heading()} · DumbMonit</title></svelte:head>

<PageHeader title={m.status_heading()} />

<Tabs {tabs} label={m.status_tabs_label()}>
	{#snippet panel(id)}
		{#if id === 'reports'}
			<ReportsTab />
		{:else if id === 'sla'}
			<SlaTab />
		{:else}
			<StatusPagesSection />
		{/if}
	{/snippet}
</Tabs>
