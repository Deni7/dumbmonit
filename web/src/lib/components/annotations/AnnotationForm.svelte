<script lang="ts">
	/**
	 * Add a change marker to a monitor's charts: a title, a time (now by
	 * default), an optional end for something that lasted a while, an optional
	 * text and link. A monitor page offers "this monitor only" or, outside team
	 * restrictions, "all monitors".
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { createAnnotation, type Annotation, type AnnotationScope } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { fromLocalInput, toLocalInput } from '#lib/components/oncall/helpers.js';
	import { Button, DateTimeField, ErrorNotice, Field } from '#lib/ui/index.js';

	interface Props {
		targetId: number;
		oncreated: (created: Annotation) => void;
		oncancel: () => void;
	}

	let { targetId, oncreated, oncancel }: Props = $props();

	const prefix = 'annotation-new';
	let title = $state('');
	let at = $state(toLocalInput(new Date()));
	let until = $state('');
	let text = $state('');
	let link = $state('');
	let everywhere = $state(false);

	let titleError = $state<string | null>(null);
	let atError = $state<string | null>(null);
	let untilError = $state<string | null>(null);
	let linkError = $state<string | null>(null);
	let saving = $state(false);
	let error = $state<unknown>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		const startIso = fromLocalInput(at);
		const endIso = until ? fromLocalInput(until) : null;
		titleError = title.trim() ? null : m.w5annot_form_title_required();
		atError = startIso ? null : m.w5annot_form_when_invalid();
		untilError =
			until && (!endIso || (startIso && endIso < startIso)) ? m.w5annot_form_until_invalid() : null;
		linkError = !link.trim() || /^https?:\/\/\S+$/.test(link.trim()) ? null : m.w5annot_form_link_invalid();
		if (titleError || atError || untilError || linkError || !startIso) return;

		const scope: AnnotationScope = everywhere ? { kind: 'global' } : { kind: 'monitors', targets: [targetId] };
		saving = true;
		try {
			oncreated(
				await createAnnotation({
					title: title.trim(),
					at: startIso,
					until: endIso,
					text: text.trim(),
					link: link.trim(),
					scope,
					source: 'ui'
				})
			);
		} catch (cause) {
			error = cause;
		} finally {
			saving = false;
		}
	}
</script>

<form class="grid gap-4 rounded-lg border border-line bg-surface p-4" onsubmit={submit} novalidate>
	<h3 class="text-sm font-semibold text-ink">{m.w5annot_form_heading()}</h3>
	<Field label={m.w5annot_form_title()} for={`${prefix}-title`} error={titleError} required>
		<input
			id={`${prefix}-title`}
			type="text"
			class="input"
			bind:value={title}
			oninput={() => (titleError = null)}
			placeholder={m.w5annot_form_title_placeholder()}
			maxlength="200"
			autocomplete="off"
			disabled={saving}
			aria-invalid={titleError ? 'true' : undefined}
		/>
	</Field>
	<div class="grid gap-4 sm:grid-cols-2">
		<Field label={m.w5annot_form_when()} for={`${prefix}-at`} error={atError} required>
			<DateTimeField id={`${prefix}-at`} bind:value={at} label={m.w5annot_form_when()} disabled={saving} invalid={!!atError} />
		</Field>
		<Field label={m.w5annot_form_until()} for={`${prefix}-until`} error={untilError} help={m.w5annot_form_until_help()}>
			<DateTimeField id={`${prefix}-until`} bind:value={until} min={at} label={m.w5annot_form_until()} clearable disabled={saving} invalid={!!untilError} />
		</Field>
	</div>
	<Field label={m.w5annot_form_text()} for={`${prefix}-text`}>
		<textarea id={`${prefix}-text`} class="input min-h-10" rows="2" bind:value={text} maxlength="4000" disabled={saving}></textarea>
	</Field>
	<Field label={m.w5annot_form_link()} for={`${prefix}-link`} error={linkError} help={m.w5annot_form_link_help()}>
		<input
			id={`${prefix}-link`}
			type="url"
			class="input"
			bind:value={link}
			oninput={() => (linkError = null)}
			placeholder="https://"
			maxlength="500"
			autocomplete="off"
			disabled={saving}
			aria-invalid={linkError ? 'true' : undefined}
		/>
	</Field>
	{#if !auth.scoped}
		<Field label={m.w5annot_form_scope()} for={`${prefix}-scope`}>
			<select id={`${prefix}-scope`} class="input" bind:value={everywhere} disabled={saving}>
				<option value={false}>{m.w5annot_form_scope_this()}</option>
				<option value={true}>{m.w5annot_form_scope_all()}</option>
			</select>
		</Field>
	{/if}
	{#if error}
		<ErrorNotice {error} title={m.w5annot_form_error()} />
	{/if}
	<div class="flex flex-wrap items-center gap-2">
		<Button type="submit" variant="primary" loading={saving}>{m.w5annot_form_save()}</Button>
		<Button type="button" onclick={oncancel} disabled={saving}>{m.w5annot_form_cancel()}</Button>
	</div>
</form>
