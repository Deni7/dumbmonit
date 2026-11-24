<script lang="ts">
	import { docTitle } from '#lib/stores/page-title.svelte.js';
	/**
	 * Overview: the night-sky banner with its pigeon and one line of status,
	 * three figures, what needs attention beside the incidents of the week and
	 * what is coming up, and the fleet's response time.
	 * See EnterpriseOverview.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import EnterpriseOverview from '#lib/components/enterprise/EnterpriseOverview.svelte';
</script>

<svelte:head><title>{docTitle(m.overview_page_title())}</title></svelte:head>

<EnterpriseOverview />
