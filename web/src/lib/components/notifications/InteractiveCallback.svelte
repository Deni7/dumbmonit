<script lang="ts">
	/**
	 * Where a platform sends the presses of interactive buttons. Slack needs the
	 * address pasted into the app (Interactivity Request URL); DumbMonit sets it
	 * itself as the Telegram bot's webhook. The address only exists once the
	 * channel is saved with the switch on and the instance has a public URL.
	 */
	import type { Channel } from '#lib/api/index.js';
	import { INTERACTIVE_KEY } from '#lib/api/index.js';
	import { CopyBlock } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		kind: string;
		/** The saved channel, when editing. */
		channel?: Channel;
	}

	let { kind, channel }: Props = $props();

	/** Interactive in what is saved (not just switched on in the form). */
	const savedOn = $derived(channel !== undefined && channel.kind === kind && channel.settings?.[INTERACTIVE_KEY] === true);
	const url = $derived(savedOn ? (channel?.callback_url ?? '') : '');
</script>

<div class="grid gap-2 rounded-lg border border-line bg-surface-2 px-3.5 py-3">
	{#if url}
		<p class="text-sm font-medium text-ink">
			{kind === 'slack' ? m.w5chat_interactive_slack_url_label() : m.w5chat_interactive_telegram_url_label()}
		</p>
		<p class="text-[0.8125rem] text-ink-2">
			{kind === 'slack' ? m.w5chat_interactive_slack_url_help() : m.w5chat_interactive_telegram_url_help()}
		</p>
		<CopyBlock value={url} />
	{:else if savedOn}
		<p class="text-[0.8125rem] text-ink-2">{m.w5chat_interactive_no_public_url()}</p>
	{:else}
		<p class="text-[0.8125rem] text-ink-2">
			{kind === 'slack' ? m.w5chat_interactive_after_save_slack() : m.w5chat_interactive_after_save_telegram()}
		</p>
	{/if}
</div>
