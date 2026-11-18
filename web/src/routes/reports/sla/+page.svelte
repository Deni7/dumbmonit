<script lang="ts">
	/** Moved: the availability report is the "SLA" page. Old links (query included) land there. */
	import { goto } from '#lib/navigation.js';
	import { page } from '$app/state';

	$effect(() => {
		void goto(`/sla${page.url.search}`, { replace: true });
	});
</script>
