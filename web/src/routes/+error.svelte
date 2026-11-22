<script lang="ts">
	import { docTitle } from '#lib/stores/page-title.svelte.js';
	import { page } from '$app/state';
	import { Compass } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { Button, EmptyState } from '#lib/ui/index.js';
	import { palette } from '#lib/stores/palette.svelte.js';

	const notFound = $derived(page.status === 404);
	const forbidden = $derived(page.status === 403);
	const serverError = $derived(page.status >= 500);
	const heading = $derived(
		notFound ? m.error_page_title_not_found() : forbidden ? m.error_page_title_forbidden() : m.error_page_title_generic({ status: page.status })
	);
</script>

<svelte:head><title>{docTitle(heading)}</title></svelte:head>

<div class="mx-auto max-w-lg py-16">
	<EmptyState
		icon={Compass}
		mascot={notFound ? 'dizzy' : undefined}
		title={notFound ? m.error_page_heading_not_found() : forbidden ? m.error_page_heading_forbidden() : m.error_page_title_generic({ status: page.status })}
		description={notFound ? m.error_page_description_not_found() : forbidden ? m.error_page_description_forbidden() : (page.error?.message ?? m.error_page_description_fallback())}
	>
		{#snippet action()}
			<div class="flex flex-wrap items-center justify-center gap-2">
				<Button href="/" variant="primary">{m.error_page_back()}</Button>
				{#if serverError}
					<Button variant="secondary" onclick={() => location.reload()}>{m.w2global_error_try_again()}</Button>
				{:else if notFound}
					<Button variant="secondary" onclick={() => history.back()}>{m.w2global_error_previous()}</Button>
					<Button variant="secondary" onclick={() => palette.open()}>{m.w2global_error_search()}</Button>
				{/if}
			</div>
		{/snippet}
	</EmptyState>
</div>
