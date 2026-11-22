<script lang="ts">
	/**
	 * Optional title and body templates of a channel. The preview is rendered
	 * by the server on a sample alert, with the same engine and escaping as the
	 * real messages, so what shows here is what the service receives. Empty
	 * fields mean the default: the usual message, unchanged.
	 */
	import { untrack } from 'svelte';
	import { RotateCcw } from 'lucide-svelte';
	import { previewMessageTemplate, toApiError, type MessageTemplateFormat, type TemplatePreview } from '#lib/api/index.js';
	import { Button, Disclosure, Field } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		kind: string;
		format: MessageTemplateFormat;
		title: string;
		body: string;
		disabled?: boolean;
		onchange: (title: string, body: string) => void;
	}

	let { kind, format, title, body, disabled = false, onchange }: Props = $props();

	/** Offered as insert buttons; any device tag works too (`tags.<key>`). */
	const VARIABLES = ['monitor', 'severity', 'state', 'summary', 'value', 'threshold', 'rule', 'url', 'started_at', 'duration', 'site', 'count', 'date', 'title', 'tags.room'];
	const DEFAULT_TITLE = '{{title}}';
	const DEFAULT_BODY = '{{summary}}\n{{url}}';

	const custom = $derived(title.trim() !== '' || body.trim() !== '');
	let open = $state(untrack(() => title.trim() !== '' || body.trim() !== ''));
	let preview = $state<TemplatePreview | null>(null);
	let previewError = $state<string | null>(null);
	let loading = $state(false);
	let titleBox = $state<HTMLInputElement | null>(null);
	let bodyBox = $state<HTMLTextAreaElement | null>(null);
	let lastFocus: 'title' | 'body' = 'body';

	const formatHint = $derived(
		format === 'slack_mrkdwn'
			? m.w5chat_template_format_slack()
			: format === 'html'
				? m.w5chat_template_format_html()
				: format === 'telegram_markdown_v2'
					? m.w5chat_template_format_telegram()
					: m.w5chat_template_format_plain()
	);

	// The preview follows the typing, a moment after it pauses.
	$effect(() => {
		if (!open) return;
		const request = { kind, title_template: title, body_template: body };
		const controller = new AbortController();
		const timer = setTimeout(() => {
			loading = true;
			previewMessageTemplate(request, controller.signal)
				.then((result) => {
					preview = result;
					previewError = null;
				})
				.catch((cause) => {
					if (!controller.signal.aborted) previewError = toApiError(cause).message;
				})
				.finally(() => {
					if (!controller.signal.aborted) loading = false;
				});
		}, 350);
		return () => {
			clearTimeout(timer);
			controller.abort();
		};
	});

	function token(name: string): string {
		return `{{${name}}}`;
	}

	/** Inserts a variable where the cursor was in the last field used. */
	function insert(name: string) {
		const text = token(name);
		const box = lastFocus === 'title' ? titleBox : bodyBox;
		const current = lastFocus === 'title' ? title : body;
		const start = box?.selectionStart ?? current.length;
		const end = box?.selectionEnd ?? current.length;
		const next = current.slice(0, start) + text + current.slice(end);
		if (lastFocus === 'title') onchange(next, body);
		else onchange(title, next);
		requestAnimationFrame(() => {
			box?.focus();
			box?.setSelectionRange(start + text.length, start + text.length);
		});
	}
</script>

<Disclosure
	title={m.w5chat_template_heading()}
	summary={custom ? m.w5chat_template_state_custom() : m.w5chat_template_state_default()}
	bind:open
>
	<div class="grid gap-4 px-4 pt-1 pb-4">
		<p class="text-[0.8125rem] text-ink-2">{m.w5chat_template_intro()} {formatHint}</p>

		<Field label={m.w5chat_template_title_label()} for="channel-template-title">
			<input
				id="channel-template-title"
				type="text"
				class="input font-mono"
				bind:this={titleBox}
				value={title}
				placeholder={DEFAULT_TITLE}
				autocomplete="off"
				spellcheck="false"
				{disabled}
				onfocus={() => (lastFocus = 'title')}
				oninput={(event) => onchange(event.currentTarget.value, body)}
			/>
		</Field>

		<Field label={m.w5chat_template_body_label()} for="channel-template-body" error={previewError}>
			<textarea
				id="channel-template-body"
				class="input min-h-28 font-mono"
				rows="5"
				bind:this={bodyBox}
				value={body}
				placeholder={DEFAULT_BODY}
				spellcheck="false"
				{disabled}
				aria-invalid={previewError ? 'true' : undefined}
				onfocus={() => (lastFocus = 'body')}
				oninput={(event) => onchange(title, event.currentTarget.value)}
			></textarea>
		</Field>

		<div class="grid gap-2" role="group" aria-labelledby="channel-template-variables">
			<p id="channel-template-variables" class="text-sm font-medium text-ink">{m.w5chat_template_variables()}</p>
			<p class="-mt-1 text-[0.8125rem] text-ink-2">{m.w5chat_template_variables_help()}</p>
			<div class="flex flex-wrap gap-1.5">
				{#each VARIABLES as name (name)}
					<button
						type="button"
						class="inline-flex min-h-10 items-center rounded-md border border-line bg-surface px-2.5 font-mono text-[0.8125rem] text-ink-2 transition-colors duration-150 enabled:hover:border-line-strong enabled:hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal disabled:opacity-45"
						aria-label={m.w5chat_template_insert_aria({ name })}
						{disabled}
						onclick={() => insert(name)}
					>
						{token(name)}
					</button>
				{/each}
			</div>
		</div>

		<section class="grid gap-1.5" aria-labelledby="channel-template-preview" aria-busy={loading}>
			<div class="flex flex-wrap items-baseline justify-between gap-2">
				<h4 id="channel-template-preview" class="text-sm font-medium text-ink">{m.w5chat_template_preview()}</h4>
				{#if loading}<span class="text-[0.8125rem] text-ink-3" role="status">{m.w5chat_template_preview_loading()}</span>{/if}
			</div>
			<p class="text-[0.8125rem] text-ink-2">{m.w5chat_template_preview_note()}</p>
			{#if preview}
				<div class="min-w-0 rounded-lg border-l-4 border-signal bg-surface-2 px-3.5 py-3">
					<p class="text-sm font-semibold break-words text-ink">{preview.title}</p>
					{#if preview.body}
						<pre class="mt-1.5 font-mono text-[0.8125rem] leading-relaxed break-words whitespace-pre-wrap text-ink-2">{preview.body}</pre>
					{/if}
				</div>
			{/if}
		</section>

		{#if custom}
			<div>
				<Button variant="ghost" size="sm" {disabled} onclick={() => onchange('', '')}>
					<RotateCcw class="size-3.5" aria-hidden="true" />
					{m.w5chat_template_reset()}
				</Button>
			</div>
		{/if}
	</div>
</Disclosure>
