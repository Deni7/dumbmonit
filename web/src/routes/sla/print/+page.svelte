<script lang="ts">
	/**
	 * The printable availability report (opened from Status › SLA): builds the
	 * report from the options in the address (the query of
	 * `GET /api/reports/sla`) and shows only the sheet, then opens the browser's
	 * print dialog (which also saves it as a PDF). A "Print" button stays on
	 * screen to print again; it is not part of the paper.
	 */
	import { tick, untrack } from 'svelte';
	import { Printer } from 'lucide-svelte';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { toApiError } from '#lib/api/index.js';
	import { getSlaReport, slaRequestFrom, type SlaReport } from '#lib/api/reports.js';
	import { Button, ErrorNotice, Skeleton } from '#lib/ui/index.js';
	import SlaSheet from '#lib/components/reports/SlaSheet.svelte';

	const request = untrack(() => slaRequestFrom(page.url.searchParams));

	let report = $state<SlaReport | null>(null);
	let error = $state<unknown>(null);

	/** `@page` follows the paper chosen for the PDF. */
	const pageRule = $derived(
		`<style>@page { size: ${request.layout?.paper === 'letter' ? 'letter' : 'A4'}; margin: 17mm 16mm 18mm; }</style>`
	);

	$effect(() => {
		const controller = new AbortController();
		getSlaReport(request, controller.signal)
			.then(async (value) => {
				report = value;
				await tick();
				// Let the logo load before the dialog takes its picture of the page.
				const images = Array.from(document.querySelectorAll<HTMLImageElement>('.sla-sheet img'));
				await Promise.all(
					images.map((image) => (image.complete ? undefined : new Promise((done) => {
						image.addEventListener('load', done, { once: true });
						image.addEventListener('error', done, { once: true });
					})))
				);
				window.print();
			})
			.catch((cause) => {
				if (cause instanceof DOMException && cause.name === 'AbortError') return;
				error = cause;
			});
		return () => controller.abort();
	});
</script>

<svelte:head>
	<title>{m.reports_sla_page_title()}</title>
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- fixed text, only the paper size varies between two literals -->
	{@html pageRule}
</svelte:head>

<main class="mx-auto w-full max-w-5xl px-4 py-6">
	{#if error}
		<ErrorNotice error={toApiError(error)} title={m.reports_print_error()} />
	{:else if report}
		<div class="mb-3 flex justify-end print:hidden">
			<Button variant="secondary" onclick={() => window.print()}>
				<Printer class="size-4" aria-hidden="true" />
				{m.reports_print_print()}
			</Button>
		</div>
		<SlaSheet {report} />
	{:else}
		<Skeleton class="h-[36rem] w-full" />
	{/if}
</main>
