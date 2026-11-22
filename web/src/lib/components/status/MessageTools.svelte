<script lang="ts">
	/**
	 * Under a message field of the incident forms: canned texts to insert, and a
	 * preview of the announcement drawn as the public page will draw it, before
	 * anything is published.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { Eye, EyeOff } from 'lucide-svelte';
	import type { IncidentKind, IncidentStatus, PublicIncident } from '#lib/api/index.js';
	import { Button } from '#lib/ui/index.js';
	import IncidentCard from './IncidentCard.svelte';
	import { insertTemplate } from './incident-view';
	import { templatesFor } from './templates';

	interface Props {
		kind: IncidentKind;
		/** The message being written. */
		body: string;
		/** The announcement with that message, as the public page would show it. */
		preview: PublicIncident;
		disabled?: boolean;
		/** A template was inserted: the status it goes with. */
		onpick?: (status: IncidentStatus) => void;
		onbody: (text: string) => void;
	}

	let { kind, body, preview, disabled = false, onpick, onbody }: Props = $props();
	let showing = $state(false);
</script>

<div class="grid gap-2">
	<div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={m.status_incidents_templates_label()}>
		<span class="text-[0.8125rem] text-ink-2">{m.status_incidents_templates_label()}</span>
		{#each templatesFor(kind) as template (template.status)}
			<Button
				variant="ghost"
				size="sm"
				{disabled}
				onclick={() => {
					onbody(insertTemplate(body, template.text()));
					onpick?.(template.status);
				}}
			>
				{template.label()}
			</Button>
		{/each}
		<Button variant="ghost" size="sm" class="sm:ml-auto" aria-pressed={showing} onclick={() => (showing = !showing)}>
			{#if showing}
				<EyeOff class="size-4" aria-hidden="true" />
				{m.status_incidents_preview_hide()}
			{:else}
				<Eye class="size-4" aria-hidden="true" />
				{m.status_incidents_preview_show()}
			{/if}
		</Button>
	</div>
	{#if showing}
		<div class="grid gap-1.5 rounded-lg border border-dashed border-line p-3">
			<p class="text-[0.8125rem] text-ink-2">{m.status_incidents_preview_note()}</p>
			<IncidentCard incident={preview} preview />
		</div>
	{/if}
</div>
