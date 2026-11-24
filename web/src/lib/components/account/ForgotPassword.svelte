<script lang="ts">
	/**
	 * "Forgot password?" inside the sign-in gate: a username or an email
	 * address, and the server emails a reset link when an account can be reset
	 * that way. The answer is the same whether an account matches or not — the
	 * screen never says which.
	 */
	import { untrack } from 'svelte';
	import { MailCheck } from 'lucide-svelte';
	import { ApiError } from '#lib/api/index.js';
	import { requestPasswordReset } from '#lib/api/accounts.js';
	import { Button, DocLink, ErrorNotice, Field } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		/** What the sign-in form already holds. */
		initial?: string;
		onback: () => void;
	}

	let { initial = '', onback }: Props = $props();

	let login = $state(untrack(() => initial));
	let loginError = $state<string | null>(null);
	let sending = $state(false);
	let sent = $state(false);
	let failure = $state<unknown>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		failure = null;
		loginError = login.trim() ? null : m.w6accounts_forgot_login_required();
		if (loginError) return;
		sending = true;
		try {
			await requestPasswordReset(login.trim());
			sent = true;
		} catch (cause) {
			failure =
				cause instanceof ApiError && cause.status === 429
					? new ApiError(m.w6accounts_forgot_too_many(), 429)
					: cause;
		} finally {
			sending = false;
		}
	}
</script>

<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_forgot_title()}</h1>

{#if sent}
	<div class="mt-4 flex gap-3 rounded-[var(--radius-card)] border border-line bg-surface-2/40 px-4 py-3" role="status">
		<MailCheck class="mt-0.5 size-5 shrink-0 text-signal-ink" aria-hidden="true" />
		<div class="min-w-0 text-sm">
			<p class="font-semibold text-ink">{m.w6accounts_forgot_sent_title()}</p>
			<p class="mt-1 text-ink-2">{m.w6accounts_forgot_sent_detail()}</p>
		</div>
	</div>
	<p class="mt-4 text-[0.8125rem] text-ink-2">{m.w6accounts_forgot_sent_none()}</p>
	<DocLink page="faq/#i-lost-the-password" label={m.w6accounts_forgot_docs()} />
	<Button variant="secondary" size="lg" class="mt-4 w-full" onclick={onback}>{m.w6accounts_forgot_back()}</Button>
{:else}
	<p class="mt-2 text-sm text-ink-2">{m.w6accounts_forgot_intro()}</p>
	<form class="mt-6 grid gap-4" onsubmit={submit} novalidate>
		<Field label={m.w6accounts_forgot_login_label()} for="forgot-login" error={loginError} help={m.w6accounts_forgot_login_help()}>
			<!-- svelte-ignore a11y_autofocus -->
			<input
				id="forgot-login"
				type="text"
				class="input"
				bind:value={login}
				autocomplete="username"
				autocapitalize="off"
				spellcheck="false"
				maxlength="254"
				autofocus
				disabled={sending}
				aria-invalid={loginError ? 'true' : undefined}
				oninput={() => (loginError = null)}
			/>
		</Field>
		{#if failure}
			<ErrorNotice title={m.w6accounts_forgot_failed()} error={failure} />
		{/if}
		<Button type="submit" variant="primary" size="lg" class="w-full" loading={sending}>{m.w6accounts_forgot_submit()}</Button>
		<Button type="button" variant="ghost" size="sm" class="justify-self-start" onclick={onback}>{m.w6accounts_forgot_back()}</Button>
	</form>
{/if}
