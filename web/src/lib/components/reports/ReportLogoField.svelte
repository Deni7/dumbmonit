<script lang="ts">
	/**
	 * The logo at the top of an SLA report: automatic (the client's, else the
	 * status page's), the client's, the status page's, one uploaded for
	 * reports, or none. Under the choice, the logo that will be used, or what
	 * is missing and where to add it (a client without a logo gets one on the
	 * Clients page, a status page in its editor). The uploaded logo goes
	 * through `PUT /api/reports/logo`, the same base64 upload as the logos of
	 * clients and status pages; PNG or JPEG only, the images a PDF embeds.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { clientLogoUrl, statusPageLogoUrl, type StatusPage } from '#lib/api/index.js';
	import { deleteReportLogo, reportLogoUrl, uploadReportLogo, type ReportLogo } from '#lib/api/reports.js';
	import { Button, ErrorNotice, Field } from '#lib/ui/index.js';

	interface Props {
		/** Bindable. */
		value: ReportLogo;
		idPrefix: string;
		/** The report's client: an id, `null` for none, `undefined` when it is set elsewhere (a scheduled report's assignment). */
		clientId?: number | null;
		/** The status page chosen to brand the report, or `null`. */
		pageId: number | null;
		pages: StatusPage[];
		disabled?: boolean;
		onchange?: (value: ReportLogo) => void;
		/** The uploaded logo was replaced or removed (a preview showing it is stale). */
		onlogochanged?: () => void;
	}

	let { value = $bindable(), idPrefix, clientId, pageId, pages, disabled = false, onchange, onlogochanged }: Props = $props();

	/** Logos above this size are refused by the server. */
	const MAX_LOGO_BYTES = 256 * 1024;

	const clients = $derived(auth.mspOn ? (auth.msp?.clients ?? []) : []);
	const client = $derived(clientId == null ? null : (clients.find((c) => c.id === clientId) ?? null));
	const page = $derived(pageId === null ? null : (pages.find((p) => p.id === pageId) ?? null));

	const OPTIONS = $derived(
		[
			{ value: 'auto' as const, label: m.reports_logo_auto() },
			...(clients.length > 0 ? [{ value: 'client' as const, label: m.reports_logo_client() }] : []),
			{ value: 'page' as const, label: m.reports_logo_page() },
			{ value: 'custom' as const, label: m.reports_logo_custom() },
			{ value: 'none' as const, label: m.reports_logo_none() }
		]
	);

	function choose(next: string) {
		const logo = (OPTIONS.find((o) => o.value === next)?.value ?? 'auto') as ReportLogo;
		value = logo;
		onchange?.(logo);
	}

	// ---- The uploaded logo ------------------------------------------------------

	let version = $state(String(Date.now()));
	/** Whether a logo has been uploaded: unknown until the preview loads or fails. */
	let hasCustom = $state<boolean | null>(null);
	let busy = $state(false);
	let error = $state<unknown>(null);
	let tooBig = $state(false);

	async function pick(event: Event & { currentTarget: HTMLInputElement }) {
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = '';
		if (!file) return;
		tooBig = file.size > MAX_LOGO_BYTES;
		if (tooBig) return;
		busy = true;
		error = null;
		try {
			const data = await new Promise<string>((resolve, reject) => {
				const reader = new FileReader();
				reader.onload = () => resolve(String(reader.result));
				reader.onerror = () => reject(reader.error);
				reader.readAsDataURL(file);
			});
			await uploadReportLogo(data);
			hasCustom = null;
			version = String(Date.now());
			if (value !== 'custom') choose('custom');
			onlogochanged?.();
		} catch (cause) {
			error = cause;
		} finally {
			busy = false;
		}
	}

	async function remove() {
		busy = true;
		error = null;
		try {
			await deleteReportLogo();
			hasCustom = false;
			version = String(Date.now());
			onlogochanged?.();
		} catch (cause) {
			error = cause;
		} finally {
			busy = false;
		}
	}

	const thumb = 'h-12 max-w-40 rounded-md border border-line bg-canvas object-contain p-1';
	const linkClass = 'font-semibold text-ink underline decoration-line underline-offset-2 hover:decoration-ink';
</script>

<div class="grid gap-2">
	<Field label={m.reports_logo_label()} for={`${idPrefix}-logo`} help={m.reports_logo_help()}>
		<select id={`${idPrefix}-logo`} class="input" {value} onchange={(event) => choose(event.currentTarget.value)} {disabled}>
			{#each OPTIONS as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
		</select>
	</Field>

	<div class="flex min-h-12 flex-wrap items-center gap-3 text-[0.8125rem] text-ink-2" aria-live="polite">
		{#if value === 'client'}
			{#if clientId === undefined}
				<span>{m.reports_logo_client_assigned()}</span>
			{:else if client === null}
				<span>{m.reports_logo_client_pick()}</span>
			{:else if client.has_logo}
				<img src={clientLogoUrl(client.id)} alt={m.settings_clients_logo_alt({ name: client.name })} class={thumb} />
			{:else}
				<span>{m.reports_logo_client_missing({ name: client.name })}</span>
				{#if auth.isAdmin}<a href={withBase('/clients')} class={linkClass}>{m.reports_logo_client_add()}</a>{/if}
			{/if}
		{:else if value === 'page'}
			{#if page === null}
				<span>{m.reports_logo_page_pick()}</span>
			{:else if page.logo_type === null}
				<span>{m.reports_logo_page_missing({ name: page.title })}</span>
				<a href={withBase(`/status/${page.id}`)} class={linkClass}>{m.reports_logo_page_add()}</a>
			{:else}
				<img src={statusPageLogoUrl(page.id, page.updated_at)} alt={m.reports_logo_page_alt({ name: page.title })} class={thumb} />
				{#if page.logo_type === 'image/webp'}<span class="font-medium text-warning-ink">{m.reports_logo_webp()}</span>{/if}
			{/if}
		{:else if value === 'custom'}
			{#key version}
				<img
					src={reportLogoUrl(version)}
					alt={m.reports_logo_custom_alt()}
					class={`${thumb} ${hasCustom ? '' : 'hidden'}`}
					onload={() => (hasCustom = true)}
					onerror={() => (hasCustom = false)}
				/>
			{/key}
			{#if hasCustom === false}<span>{m.reports_logo_custom_missing()}</span>{/if}
			{#if auth.isAdmin}
				<label
					class={`inline-flex h-10 cursor-pointer items-center rounded-lg border border-line-strong bg-surface px-4 text-sm font-semibold text-ink shadow-lift hover:border-ink-3 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-signal ${busy || disabled ? 'pointer-events-none opacity-50' : ''}`}
				>
					<input class="sr-only" type="file" accept="image/png,image/jpeg" onchange={pick} disabled={busy || disabled} />
					{hasCustom ? m.reports_logo_replace() : m.reports_logo_upload()}
				</label>
				{#if hasCustom}
					<Button variant="ghost" onclick={() => void remove()} disabled={busy || disabled}>{m.reports_logo_remove()}</Button>
				{/if}
			{/if}
		{:else if value === 'auto'}
			<span>{m.reports_logo_auto_help()}</span>
		{/if}
	</div>
	{#if tooBig}<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{m.reports_logo_too_big()}</p>{/if}
	{#if error}<ErrorNotice {error} title={m.reports_logo_error()} />{/if}
</div>
