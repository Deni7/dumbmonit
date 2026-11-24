<script lang="ts">
	/**
	 * The post-mortem of one incident, inline under its row: summary, impact,
	 * root cause, a timeline (started from the incident's updates), action
	 * items with a done box, and "Publish on the status page", offered once
	 * the incident is resolved. Plain text: the public page shows it by
	 * paragraphs. Times are typed in the browser's local time and sent in UTC.
	 */
	import { untrack } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { Plus, X } from 'lucide-svelte';
	import { deleteIncidentPostmortem, saveIncidentPostmortem, type Incident } from '#lib/api/index.js';
	import { formatDateTime } from '#lib/format.js';
	import { Button, Confirm, DateTimeField, ErrorNotice, Field, Plate, Toggle } from '#lib/ui/index.js';
	import { dateToLocal, localToIso } from './incident-history';
	import { MAX_ACTIONS, MAX_TIMELINE, cleanDraft, publishBlock, startDraft } from './postmortem';

	interface Props {
		incident: Incident;
		onchange: (incident: Incident) => void;
		onclose: () => void;
	}

	let { incident, onchange, onclose }: Props = $props();

	const prefix = untrack(() => `pm-${incident.id}`);
	const first = untrack(() => startDraft(incident.postmortem, incident.updates));

	let summary = $state(first.summary);
	let impact = $state(first.impact);
	let rootCause = $state(first.root_cause);
	/** `at` as a `datetime-local` value. */
	let timeline = $state(first.timeline.map((entry) => ({ at: dateToLocal(new Date(entry.at)), text: entry.text })));
	let actions = $state(first.actions);
	let published = $state(first.published);

	let saving = $state(false);
	let deleting = $state(false);
	let error = $state<unknown>(null);

	const draft = $derived({
		summary,
		impact,
		root_cause: rootCause,
		timeline: timeline.map((row) => ({ at: localToIso(row.at) ?? '', text: row.text })),
		actions,
		published
	});
	const block = $derived(publishBlock(incident.status, draft));
	const blockText = $derived(block === 'not_resolved' ? m.status_postmortem_publish_wait() : block === 'no_summary' ? m.status_postmortem_publish_summary() : null);

	function addStep() {
		const last = timeline[timeline.length - 1];
		timeline = [...timeline, { at: last?.at ?? dateToLocal(new Date()), text: '' }];
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		saving = true;
		try {
			const sent = cleanDraft({ ...draft, published: published && block === null });
			onchange(await saveIncidentPostmortem(incident.id, sent));
			onclose();
		} catch (cause) {
			error = cause;
		} finally {
			saving = false;
		}
	}

	async function remove() {
		deleting = true;
		error = null;
		try {
			await deleteIncidentPostmortem(incident.id);
			onchange({ ...incident, postmortem: null });
			onclose();
		} catch (cause) {
			error = cause;
		} finally {
			deleting = false;
		}
	}

	const busy = $derived(saving || deleting);
</script>

<form class="mt-3 grid gap-4 rounded-lg border border-line bg-surface p-3 sm:p-4" onsubmit={save} novalidate>
	<div class="flex flex-wrap items-center gap-2">
		<p class="text-sm font-semibold text-ink">{m.status_postmortem_title()}</p>
		{#if incident.postmortem}
			<Plate tone={incident.postmortem.published ? 'signal' : 'ghost'} label={incident.postmortem.published ? m.status_list_published() : m.status_list_draft()} />
			{#if incident.postmortem.published_at}
				<span class="text-[0.8125rem] text-ink-2">{m.status_postmortem_public_published({ date: formatDateTime(incident.postmortem.published_at) })}</span>
			{/if}
		{/if}
	</div>
	<p class="-mt-2 max-w-2xl text-[0.8125rem] text-ink-2">{m.status_postmortem_lead()}</p>

	<Field label={m.status_postmortem_summary()} for={`${prefix}-summary`}>
		<textarea id={`${prefix}-summary`} class="input min-h-10" rows="3" bind:value={summary} maxlength="20000" disabled={busy}></textarea>
	</Field>
	<div class="grid gap-4 sm:grid-cols-2">
		<Field label={m.status_postmortem_impact()} for={`${prefix}-impact`}>
			<textarea id={`${prefix}-impact`} class="input min-h-10" rows="3" bind:value={impact} maxlength="20000" disabled={busy}></textarea>
		</Field>
		<Field label={m.status_postmortem_root_cause()} for={`${prefix}-cause`}>
			<textarea id={`${prefix}-cause`} class="input min-h-10" rows="3" bind:value={rootCause} maxlength="20000" disabled={busy}></textarea>
		</Field>
	</div>

	<div class="grid gap-2">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<div>
				<p class="text-sm font-semibold text-ink">{m.status_postmortem_timeline()}</p>
				<p class="text-[0.8125rem] text-ink-2">{m.status_postmortem_timeline_help()}</p>
			</div>
			<Button type="button" variant="ghost" size="sm" onclick={addStep} disabled={busy || timeline.length >= MAX_TIMELINE}>
				<Plus class="size-4" aria-hidden="true" />
				{m.status_postmortem_timeline_add()}
			</Button>
		</div>
		<ol class="grid gap-2">
			{#each timeline as row, index (index)}
				<li class="grid gap-2 sm:grid-cols-[13rem_minmax(0,1fr)_auto] sm:items-start">
					<Field label={m.status_postmortem_timeline_when()} for={`${prefix}-t${index}-at`}>
						<DateTimeField id={`${prefix}-t${index}-at`} bind:value={row.at} disabled={busy} label={m.status_postmortem_timeline_when()} />
					</Field>
					<Field label={m.status_postmortem_timeline_what()} for={`${prefix}-t${index}-text`}>
						<textarea id={`${prefix}-t${index}-text`} class="input min-h-10" rows="1" bind:value={row.text} maxlength="2000" disabled={busy}></textarea>
					</Field>
					<button
						type="button"
						class="inline-flex size-10 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink sm:mt-[1.625rem]"
						aria-label={m.status_postmortem_timeline_remove_aria({ number: index + 1 })}
						onclick={() => (timeline = timeline.filter((_, i) => i !== index))}
						disabled={busy}
					>
						<X class="size-4" aria-hidden="true" />
					</button>
				</li>
			{/each}
		</ol>
	</div>

	<div class="grid gap-2">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<p class="text-sm font-semibold text-ink">{m.status_postmortem_actions()}</p>
			<Button type="button" variant="ghost" size="sm" onclick={() => (actions = [...actions, { text: '', done: false }])} disabled={busy || actions.length >= MAX_ACTIONS}>
				<Plus class="size-4" aria-hidden="true" />
				{m.status_postmortem_actions_add()}
			</Button>
		</div>
		<ul class="grid gap-2">
			{#each actions as item, index (index)}
				<li class="flex items-center gap-2">
					<label class="inline-flex size-10 shrink-0 items-center justify-center">
						<input type="checkbox" class="size-4 accent-[var(--c-signal)]" bind:checked={item.done} disabled={busy} aria-label={m.status_postmortem_action_done_aria({ number: index + 1 })} />
					</label>
					<input
						type="text"
						class="input min-w-0 flex-1"
						bind:value={item.text}
						maxlength="500"
						placeholder={m.status_postmortem_action_placeholder()}
						aria-label={m.status_postmortem_action_aria({ number: index + 1 })}
						disabled={busy}
					/>
					<button
						type="button"
						class="inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink"
						aria-label={m.status_postmortem_action_remove_aria({ number: index + 1 })}
						onclick={() => (actions = actions.filter((_, i) => i !== index))}
						disabled={busy}
					>
						<X class="size-4" aria-hidden="true" />
					</button>
				</li>
			{/each}
		</ul>
	</div>

	<Field label={m.status_postmortem_publish()} for={`${prefix}-publish`} help={blockText ?? m.status_postmortem_publish_help()} inline>
		<Toggle id={`${prefix}-publish`} bind:checked={published} disabled={busy || block !== null} label={m.status_postmortem_publish()} />
	</Field>

	{#if error}
		<ErrorNotice {error} title={m.status_postmortem_error()} />
	{/if}
	<div class="flex flex-wrap items-center gap-2">
		<Button type="submit" variant="secondary" loading={saving}>{m.status_postmortem_save()}</Button>
		<Button type="button" variant="ghost" onclick={onclose} disabled={busy}>{m.status_incidents_cancel()}</Button>
		{#if incident.postmortem}
			<Confirm confirmLabel={m.status_postmortem_delete_confirm()} loading={deleting} onconfirm={remove}>{m.status_postmortem_delete()}</Confirm>
		{/if}
	</div>
</form>
