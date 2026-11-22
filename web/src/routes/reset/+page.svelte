<script lang="ts">
	/**
	 * The page a reset link opens (`/reset?token=…`): choose a new password,
	 * with the same rules and strength meter as the first setup. Using the link
	 * signs the account out everywhere; then one signs in as usual.
	 *
	 * The token is read once and taken out of the address bar, so it does not
	 * linger in the history or get copied along with the page address.
	 */
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { ApiError } from '#lib/api/index.js';
	import { checkPasswordReset, completePasswordReset, type ResetCheck } from '#lib/api/accounts.js';
	import { auth, PASSWORD_MIN_LENGTH, validatePassword } from '#lib/stores/auth.svelte.js';
	import { formatDateTime } from '#lib/format.js';
	import { Button, ClickSpark, ErrorNotice, Field, Skeleton } from '#lib/ui/index.js';
	import PasswordInput from '#lib/components/settings/PasswordInput.svelte';
	import PasswordStrength from '#lib/components/settings/PasswordStrength.svelte';
	import AccountGate from '#lib/components/account/AccountGate.svelte';
	import { m } from '#lib/paraglide/messages.js';

	let token = $state('');
	let link = $state<ResetCheck | null>(null);
	let phase = $state<'checking' | 'ready' | 'invalid' | 'done'>('checking');
	let loadError = $state<unknown>(null);

	let password = $state('');
	let confirmation = $state('');
	let passwordError = $state<string | null>(null);
	let confirmError = $state<string | null>(null);
	let sending = $state(false);
	let failure = $state<unknown>(null);

	const remaining = $derived(Math.max(0, PASSWORD_MIN_LENGTH - [...password].length));
	const lengthHelp = $derived(
		password.length === 0
			? m.auth_setup_length_empty({ min: PASSWORD_MIN_LENGTH })
			: remaining > 0
				? m.auth_setup_length_remaining({ count: remaining, min: PASSWORD_MIN_LENGTH })
				: m.auth_setup_length_ok()
	);

	onMount(() => {
		token = page.url.searchParams.get('token') ?? '';
		if (token) {
			const clean = new URL(page.url);
			clean.searchParams.delete('token');
			replaceState(clean, {});
		}
		void check();
	});

	async function check() {
		loadError = null;
		phase = 'checking';
		if (!token) {
			phase = 'invalid';
			return;
		}
		try {
			link = await checkPasswordReset(token);
			phase = 'ready';
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 404) {
				phase = 'invalid';
			} else {
				loadError = cause;
				phase = 'ready';
			}
		}
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		failure = null;
		passwordError = validatePassword(password);
		confirmError = !passwordError && password !== confirmation ? m.auth_setup_mismatch() : null;
		if (passwordError || confirmError) return;
		sending = true;
		try {
			await completePasswordReset(token, password);
			password = '';
			confirmation = '';
			phase = 'done';
			// This browser's own session, if any, was closed with the others.
			void auth.refresh();
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 404) {
				phase = 'invalid';
			} else if (cause instanceof ApiError && cause.status === 400) {
				passwordError = cause.message;
			} else {
				failure = cause;
			}
		} finally {
			sending = false;
		}
	}
</script>

<svelte:head><title>{m.w6accounts_reset_page_title()}</title></svelte:head>

<AccountGate mood={phase === 'invalid' ? 'dizzy' : phase === 'done' ? 'happy' : 'watch'}>
	{#if phase === 'checking'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_reset_title()}</h1>
		<Skeleton class="mt-6 h-10 w-full" rows={3} />
	{:else if phase === 'invalid'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_reset_invalid_title()}</h1>
		<p class="mt-2 text-sm text-ink-2">{m.w6accounts_reset_invalid_detail()}</p>
		<Button href="/login" variant="primary" size="lg" class="mt-6 w-full">{m.w6accounts_reset_to_login()}</Button>
	{:else if phase === 'done'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_reset_done_title()}</h1>
		<p class="mt-2 text-sm text-ink-2" role="status">{m.w6accounts_reset_done_detail()}</p>
		<Button href="/login" variant="primary" size="lg" class="mt-6 w-full">{m.w6accounts_reset_to_login()}</Button>
	{:else}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_reset_title()}</h1>
		{#if link}
			<p class="mt-2 text-sm text-ink-2">
				{m.w6accounts_reset_intro({ username: link.username, until: formatDateTime(link.expires_at) })}
			</p>
		{/if}
		{#if loadError}
			<ErrorNotice class="mt-5" title={m.w6accounts_reset_check_failed()} error={loadError} onretry={() => void check()} />
		{:else}
			<form class="mt-6 grid gap-4" onsubmit={submit} novalidate>
				<!-- Lets password managers file the new password under the right account. -->
				<input type="text" class="sr-only" autocomplete="username" value={link?.username ?? ''} readonly tabindex="-1" aria-hidden="true" />
				<Field label={m.w6accounts_reset_password()} for="new-password" error={passwordError} help={lengthHelp}>
					<PasswordInput
						id="new-password"
						bind:value={password}
						autocomplete="new-password"
						disabled={sending}
						invalid={passwordError !== null}
						oninput={() => (passwordError = null)}
					/>
					<PasswordStrength value={password} />
				</Field>
				<Field label={m.auth_setup_confirm()} for="confirm-password" error={confirmError}>
					<PasswordInput
						id="confirm-password"
						bind:value={confirmation}
						autocomplete="new-password"
						disabled={sending}
						invalid={confirmError !== null}
						oninput={() => (confirmError = null)}
					/>
				</Field>
				{#if failure}
					<ErrorNotice title={m.w6accounts_reset_failed()} error={failure} />
				{/if}
				<ClickSpark class="w-full">
					<Button type="submit" variant="primary" size="lg" class="w-full" loading={sending}>{m.w6accounts_reset_submit()}</Button>
				</ClickSpark>
				<p class="text-[0.8125rem] text-ink-2">{m.w6accounts_reset_note()}</p>
			</form>
		{/if}
	{/if}
</AccountGate>
