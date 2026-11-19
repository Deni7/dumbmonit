<script lang="ts">
	/** Moved: the availability report is the "SLA" tab of Status. Old links (query included) land there. */
	import { goto } from '#lib/navigation.js';
	import { page } from '$app/state';
	import { tabUrl } from '#lib/tab-url.js';

	$effect(() => {
		void goto(tabUrl('/status', 'sla', page.url), { replace: true });
	});
</script>
