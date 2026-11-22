<script lang="ts">
	/**
	 * "From a template", above the device types in the add picker: the saved
	 * monitor templates, each one a way into its kind's form, prefilled like a
	 * copy (`TargetForm` with `template`). Administrators rename and delete
	 * them here. Nothing is shown while there is no template (or the list
	 * cannot be read): the picker below is the way in then.
	 */
	import { Check, LayoutTemplate, Pencil, Trash2, X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { CollectorInfo } from '#lib/api/index.js';
	import {
		deleteMonitorTemplate,
		listMonitorTemplates,
		renameMonitorTemplate,
		type MonitorTemplate
	} from '#lib/api/monitorTemplates.js';
	import { formatDuration } from '#lib/format.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Confirm, ErrorNotice } from '#lib/ui/index.js';
	import KindIcon from '#lib/components/KindIcon.svelte';

	interface Props {
		collectors: CollectorInfo[];
		onuse: (template: MonitorTemplate) => void;
	}

	let { collectors, onuse }: Props = $props();

	let templates = $state<MonitorTemplate[]>([]);
	let error = $state<unknown>(null);
	/** The template being renamed, and the name typed so far. */
	let renaming = $state<{ id: number; name: string } | null>(null);
	let busy = $state<number | null>(null);

	$effect(() => {
		const controller = new AbortController();
		listMonitorTemplates(controller.signal)
			.then((list) => (templates = list))
			.catch(() => (templates = []));
		return () => controller.abort();
	});

	const labels = $derived(new Map(collectors.map((c) => [c.kind, c.label])));
	/** A template whose kind this server no longer knows cannot fill a form. */
	const usable = (template: MonitorTemplate) => labels.has(template.kind);

	async function rename(event: SubmitEvent) {
		event.preventDefault();
		const draft = renaming;
		if (!draft || !draft.name.trim() || busy !== null) return;
		busy = draft.id;
		error = null;
		try {
			const saved = await renameMonitorTemplate(draft.id, draft.name.trim());
			templates = templates.map((t) => (t.id === saved.id ? saved : t));
			renaming = null;
		} catch (cause) {
			error = cause;
		} finally {
			busy = null;
		}
	}

	async function remove(template: MonitorTemplate) {
		busy = template.id;
		error = null;
		try {
			await deleteMonitorTemplate(template.id);
			templates = templates.filter((t) => t.id !== template.id);
		} catch (cause) {
			error = cause;
		} finally {
			busy = null;
		}
	}

	const iconButton =
		'inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal disabled:opacity-45';
</script>

{#if templates.length > 0}
	<section aria-labelledby="templates-title" class="mb-6">
		<h3 id="templates-title" class="mb-2 flex items-center gap-2 text-[0.9375rem] font-semibold text-ink">
			<LayoutTemplate class="size-4 text-ink-2" aria-hidden="true" />
			{m.w5monitors_templates_title()}
		</h3>
		{#if error}
			<ErrorNotice {error} title={m.w5monitors_templates_error()} class="mb-2" />
		{/if}
		<ul class="flex flex-col divide-y divide-line overflow-hidden rounded-[var(--radius-card)] border border-line bg-surface">
			{#each templates as template (template.id)}
				<li class="flex min-w-0 items-center gap-2 px-3 py-1.5">
					{#if renaming?.id === template.id}
						<form class="flex min-w-0 flex-1 items-center gap-1" onsubmit={rename}>
							<input
								class="input min-w-0 flex-1"
								aria-label={m.w5monitors_template_name_label()}
								maxlength="80"
								bind:value={renaming.name}
								onkeydown={(e) => {
									if (e.key === 'Escape') {
										e.preventDefault();
										renaming = null;
									}
								}}
							/>
							<button type="submit" class={iconButton} aria-label={m.w5monitors_templates_rename_save()} disabled={busy !== null || !renaming.name.trim()}>
								<Check class="size-4" aria-hidden="true" />
							</button>
							<button type="button" class={iconButton} aria-label={m.w5monitors_bulkedit_cancel()} onclick={() => (renaming = null)}>
								<X class="size-4" aria-hidden="true" />
							</button>
						</form>
					{:else}
						<button
							type="button"
							class="flex min-h-10 min-w-0 flex-1 items-center gap-3 rounded-md px-1 text-left hover:bg-surface-2/60 focus-visible:outline-2 focus-visible:outline-signal disabled:cursor-not-allowed disabled:opacity-60"
							disabled={!usable(template)}
							onclick={() => onuse(template)}
						>
							<KindIcon kind={template.kind} class="size-4 shrink-0 text-ink-2" />
							<span class="min-w-0 flex-1">
								<span class="block truncate text-sm font-semibold text-ink">{template.name}</span>
								<span class="block truncate text-[0.8125rem] text-ink-2">
									{usable(template)
										? m.w5monitors_templates_summary({ kind: labels.get(template.kind) ?? template.kind, interval: formatDuration(template.interval_secs) })
										: m.w5monitors_templates_unknown_kind({ kind: template.kind })}
								</span>
							</span>
						</button>
						{#if auth.isAdmin}
							<button type="button" class={iconButton} aria-label={m.w5monitors_templates_rename({ name: template.name })} disabled={busy !== null} onclick={() => (renaming = { id: template.id, name: template.name })}>
								<Pencil class="size-4" aria-hidden="true" />
							</button>
							<Confirm size="sm" variant="danger" confirmLabel={m.w5monitors_templates_delete_confirm()} loading={busy === template.id} disabled={busy !== null} onconfirm={() => remove(template)}>
								<Trash2 class="size-4" aria-hidden="true" />
								<span class="sr-only">{m.w5monitors_templates_delete({ name: template.name })}</span>
							</Confirm>
						{/if}
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}
