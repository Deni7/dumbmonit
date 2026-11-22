<script lang="ts">
	/**
	 * Settings → Team → Audit log (administrators): who did what, and from where.
	 *
	 * Filters (account, event, address, period) are applied by the server, the
	 * list grows with "Load more" (cursor on the id of the last entry read), and
	 * the same filters feed the CSV and JSON exports, which are plain download
	 * links to the administrator's session.
	 */
	import { Download } from 'lucide-svelte';
	import { untrack } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { AuditEntry } from '#lib/api/index.js';
	import { auditExportUrl, listAuditLog, type AuditFilter } from '#lib/api/totp.js';
	import { formatDateTime } from '#lib/format.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, DocLink, ErrorNotice, Field, Panel, Plate, Skeleton } from '#lib/ui/index.js';
	import { auditActionFailed, auditActionLabel, knownAuditActions } from './auditActions';

	const PAGE = 100;

	let entries = $state<AuditEntry[] | null>(null);
	let error = $state<unknown>(null);
	let hasMore = $state(false);
	let loadingMore = $state(false);

	// What the form holds, and what the list currently shows.
	let draft = $state<AuditFilter>({ actor: '', action: '', ip: '', since: '', until: '' });
	let applied = $state<AuditFilter>({});

	const filtered = $derived(Object.values(applied).some((value) => !!value));
	const actions = $derived(
		knownAuditActions()
			.map((action) => ({ action, label: auditActionLabel(action) }))
			.sort((a, b) => a.label.localeCompare(b.label))
	);

	async function load(signal?: AbortSignal) {
		error = null;
		try {
			const page = await listAuditLog(applied, { limit: PAGE, signal });
			entries = page;
			hasMore = page.length === PAGE;
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		}
	}

	async function more() {
		const last = entries?.at(-1);
		if (!last || loadingMore) return;
		loadingMore = true;
		try {
			const page = await listAuditLog(applied, { limit: PAGE, before: last.id });
			entries = [...(entries ?? []), ...page];
			hasMore = page.length === PAGE;
		} catch (cause) {
			error = cause;
		} finally {
			loadingMore = false;
		}
	}

	function apply(event: SubmitEvent) {
		event.preventDefault();
		applied = { ...draft };
		entries = null;
		void load();
	}

	function reset() {
		draft = { actor: '', action: '', ip: '', since: '', until: '' };
		applied = {};
		entries = null;
		void load();
	}

	$effect(() => {
		if (!auth.isAdmin) return;
		const controller = new AbortController();
		untrack(() => void load(controller.signal));
		return () => controller.abort();
	});
</script>

{#if auth.isAdmin}
	<Panel id="audit" title={m.w2ops_audit_title()} description={m.w2ops_audit_description()}>
		{#snippet aside()}
			<DocLink page="using/settings/#audit-log" />
		{/snippet}

		<form class="grid gap-3 sm:grid-cols-2 lg:grid-cols-5" onsubmit={apply} novalidate>
			<Field label={m.w2ops_audit_filter_actor()} for="audit-actor">
				<input id="audit-actor" type="text" class="input" bind:value={draft.actor} autocomplete="off" />
			</Field>
			<Field label={m.w2ops_audit_filter_action()} for="audit-action">
				<select id="audit-action" class="input" bind:value={draft.action}>
					<option value="">{m.w2ops_audit_filter_action_any()}</option>
					{#each actions as option (option.action)}
						<option value={option.action}>{option.label}</option>
					{/each}
				</select>
			</Field>
			<Field label={m.w2ops_audit_filter_ip()} for="audit-ip">
				<input id="audit-ip" type="text" class="input tnum font-mono" bind:value={draft.ip} autocomplete="off" inputmode="text" />
			</Field>
			<Field label={m.w2ops_audit_filter_since()} for="audit-since">
				<input id="audit-since" type="date" class="input" bind:value={draft.since} />
			</Field>
			<Field label={m.w2ops_audit_filter_until()} for="audit-until">
				<input id="audit-until" type="date" class="input" bind:value={draft.until} />
			</Field>
			<div class="flex flex-wrap items-center gap-2 sm:col-span-2 lg:col-span-5">
				<Button type="submit" variant="secondary" size="sm">{m.w2ops_audit_apply()}</Button>
				{#if filtered}
					<Button type="button" variant="ghost" size="sm" onclick={reset}>{m.w2ops_audit_reset()}</Button>
				{/if}
				<span class="grow"></span>
				<Button variant="ghost" size="sm" href={auditExportUrl('csv', applied)} download>
					<Download class="size-4" aria-hidden="true" />
					{m.w2ops_audit_export_csv()}
				</Button>
				<Button variant="ghost" size="sm" href={auditExportUrl('json', applied)} download>
					<Download class="size-4" aria-hidden="true" />
					{m.w2ops_audit_export_json()}
				</Button>
			</div>
		</form>

		{#if error}
			<div class="mt-3"><ErrorNotice {error} title={m.w2ops_audit_error()} onretry={() => void load()} /></div>
		{:else if entries === null}
			<Skeleton class="mt-3 h-24 w-full" />
		{:else if entries.length === 0}
			<p class="mt-3 text-[0.8125rem] text-ink-2">{filtered ? m.w2ops_audit_empty_filtered() : m.w2ops_audit_empty()}</p>
		{:else}
			<div class="mt-3 overflow-x-auto rounded-lg border border-line">
				<table class="table-stack w-full text-[0.8125rem]">
					<thead class="bg-surface-2 text-left text-ink-2">
						<tr>
							<th class="px-3 py-2 font-semibold">{m.w2ops_audit_col_when()}</th>
							<th class="px-3 py-2 font-semibold">{m.w2ops_audit_col_event()}</th>
							<th class="px-3 py-2 font-semibold">{m.w2ops_audit_col_who()}</th>
							<th class="px-3 py-2 font-semibold">{m.w2ops_audit_col_subject()}</th>
							<th class="px-3 py-2 font-semibold">{m.w2ops_audit_col_from()}</th>
						</tr>
					</thead>
					<tbody>
						{#each entries as entry (entry.id)}
							<tr class="border-t border-line">
								<td class="tnum px-3 py-1.5 whitespace-nowrap text-ink-2">{formatDateTime(entry.at)}</td>
								<td class="px-3 py-1.5" data-label={m.w2ops_audit_col_event()}>
									<Plate size="sm" tone={auditActionFailed(entry.action) ? 'warning' : 'ghost'} label={auditActionLabel(entry.action)} />
								</td>
								<td class="px-3 py-1.5 font-mono" data-label={m.w2ops_audit_col_who()}>{entry.actor ?? '—'}</td>
								<td class="px-3 py-1.5 font-mono break-all" data-label={m.w2ops_audit_col_subject()}>{entry.subject ?? '—'}</td>
								<td class="tnum px-3 py-1.5 font-mono text-ink-2" data-label={m.w2ops_audit_col_from()}>{entry.ip ?? '—'}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			{#if hasMore}
				<div class="mt-3">
					<Button variant="secondary" size="sm" loading={loadingMore} onclick={more}>{m.w2ops_audit_load_more()}</Button>
				</div>
			{/if}
		{/if}
	</Panel>
{/if}
