<script lang="ts">
	/**
	 * Overview: the night-sky banner with its pigeon and one line of status,
	 * four figures, what needs attention, the fleet health and two charts.
	 * See EnterpriseOverview.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import EnterpriseOverview from '#lib/components/enterprise/EnterpriseOverview.svelte';
</script>

<svelte:head><title>{m.overview_page_title()} · DumbMonit</title></svelte:head>

<EnterpriseOverview />
