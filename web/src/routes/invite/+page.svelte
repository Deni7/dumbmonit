<script lang="ts">
	/**
	 * The page an invitation opens (`/invite?token=…`): choose a username
	 * (suggested from the address), a display name and a password — or sign in
	 * with single sign-on when the instance has it. The account gets the role,
	 * address and teams of the invitation; a password account is signed in
	 * right away.
	 *
	 * Like `/reset`, the token is taken out of the address bar once read.
	 */
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { KeyRound } from 'lucide-svelte';
	import { ApiError } from '#lib/api/index.js';
	import { acceptInvitation, checkInvitation, type InvitationCheck } from '#lib/api/accounts.js';
	import { auth, PASSWORD_MIN_LENGTH, validatePassword } from '#lib/stores/auth.svelte.js';
	import { goto } from '#lib/navigation.js';
	import { formatDateTime } from '#lib/format.js';
	import { Button, ClickSpark, ErrorNotice, Field, Skeleton } from '#lib/ui/index.js';
	import PasswordInput from '#lib/components/settings/PasswordInput.svelte';
	import PasswordStrength from '#lib/components/settings/PasswordStrength.svelte';
	import AccountGate from '#lib/components/account/AccountGate.svelte';
	import { m } from '#lib/paraglide/messages.js';

	let token = $state('');
	let invitation = $state<InvitationCheck | null>(null);
	let phase = $state<'checking' | 'ready' | 'invalid' | 'sso'>('checking');
	let loadError = $state<unknown>(null);

	let username = $state('');
	let displayName = $state('');
	let password = $state('');
	let confirmation = $state('');
	let errors = $state<{ username?: string; password?: string; confirm?: string }>({});
	let sending = $state(false);
	let failure = $state<unknown>(null);

	const ROLE_LABELS: Record<InvitationCheck['role'], () => string> = {
		admin: () => m.settings_users_role_admin(),
		operator: () => m.settings_users_role_operator(),
		viewer: () => m.settings_users_role_viewer()
	};

	const remaining = $derived(Math.max(0, PASSWORD_MIN_LENGTH - [...password].length));
	const lengthHelp = $derived(
		password.length === 0
			? m.auth_setup_length_empty({ min: PASSWORD_MIN_LENGTH })
			: remaining > 0
				? m.auth_setup_length_remaining({ count: remaining, min: PASSWORD_MIN_LENGTH })
				: m.auth_setup_length_ok()
	);

	/** Single sign-on buttons, as on the sign-in screen. */
	const providers = $derived([
		...(auth.oidc.enabled ? [{ label: auth.oidc.provider_name, href: auth.oidc.login_url }] : []),
		...(auth.saml.enabled ? [{ label: auth.saml.label, href: '/api/auth/saml/start' }] : [])
	]);

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
			invitation = await checkInvitation(token);
			username = invitation.suggested_username;
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

	function refused(cause: unknown): void {
		if (cause instanceof ApiError && cause.status === 404) {
			phase = 'invalid';
		} else if (cause instanceof ApiError && cause.status === 409) {
			errors = { username: cause.message };
		} else if (cause instanceof ApiError && cause.status === 400 && /password/i.test(cause.message)) {
			errors = { password: cause.message };
		} else if (cause instanceof ApiError && cause.status === 400 && /username/i.test(cause.message)) {
			errors = { username: cause.message };
		} else {
			failure = cause;
		}
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		failure = null;
		const name = username.trim();
		const found: typeof errors = {};
		if (!name) found.username = m.auth_setup_username_required();
		else if (/\s/.test(name)) found.username = m.auth_setup_username_spaces();
		const problem = validatePassword(password);
		if (problem) found.password = problem;
		else if (password !== confirmation) found.confirm = m.auth_setup_mismatch();
		errors = found;
		if (Object.keys(found).length > 0) return;

		sending = true;
		try {
			const created = await acceptInvitation({ token, username: name, display_name: displayName.trim(), password });
			// The account exists: sign in with what was just typed. A refused
			// sign-in leaves the sign-in screen to say why.
			try {
				await auth.login(created.username, password);
				await goto('/', { replace: true });
			} catch {
				await goto('/login', { replace: true });
			}
		} catch (cause) {
			refused(cause);
		} finally {
			sending = false;
		}
	}

	async function withProvider(href: string) {
		failure = null;
		sending = true;
		try {
			await acceptInvitation({ token, display_name: displayName.trim(), sso: true });
			phase = 'sso';
			// A full navigation: the server sends the browser to the provider.
			globalThis.location?.assign(href);
		} catch (cause) {
			refused(cause);
		} finally {
			sending = false;
		}
	}
</script>

<svelte:head><title>{m.w6accounts_invite_page_title()}</title></svelte:head>

<AccountGate mood={phase === 'invalid' ? 'dizzy' : 'happy'}>
	{#if phase === 'checking'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_invite_title()}</h1>
		<div role="status">
			<p class="sr-only">{m.w6design_gate_checking()}</p>
			<Skeleton class="mt-6 h-10 w-full" rows={4} />
		</div>
	{:else if phase === 'invalid'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_invite_invalid_title()}</h1>
		<p class="mt-2 text-sm text-ink-2">{m.w6accounts_invite_invalid_detail()}</p>
		<Button href="/login" variant="secondary" size="lg" class="mt-6 w-full">{m.w6accounts_reset_to_login()}</Button>
	{:else if phase === 'sso'}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_invite_title()}</h1>
		<p class="mt-2 text-sm text-ink-2" role="status">{m.w6accounts_invite_sso_leaving()}</p>
	{:else}
		<h1 id="gate-title" class="display mt-6 text-[1.75rem] text-ink sm:text-3xl">{m.w6accounts_invite_title()}</h1>
		{#if invitation}
			<p class="mt-2 text-sm text-ink-2">
				{m.w6accounts_invite_intro({ inviter: invitation.invited_by, role: ROLE_LABELS[invitation.role]?.() ?? invitation.role })}
			</p>
			<p class="mt-1 text-[0.8125rem] break-all text-ink-2">
				{m.w6accounts_invite_for({ email: invitation.email, until: formatDateTime(invitation.expires_at) })}
			</p>
		{/if}

		{#if loadError}
			<ErrorNotice class="mt-5" title={m.w6accounts_invite_check_failed()} error={loadError} onretry={() => void check()} />
		{:else if invitation}
			<form class="mt-6 grid gap-4" onsubmit={submit} novalidate>
				<Field label={m.settings_users_display_name()} for="invite-display-name" help={m.settings_users_display_help()}>
					<input id="invite-display-name" type="text" class="input" bind:value={displayName} autocomplete="name" maxlength="120" disabled={sending} />
				</Field>

				{#if invitation.sso && providers.length > 0}
					<div class="grid gap-2">
						{#each providers as provider (provider.href)}
							<Button variant="secondary" size="lg" class="w-full" disabled={sending} onclick={() => void withProvider(provider.href)}>
								<KeyRound class="size-4" aria-hidden="true" />
								{m.auth_login_sso_button({ provider: provider.label })}
							</Button>
						{/each}
						<p class="text-[0.8125rem] text-ink-2">{m.w6accounts_invite_sso_help({ email: invitation.email })}</p>
					</div>
					<div class="flex items-center gap-3 text-[0.75rem] font-semibold text-ink-3" aria-hidden="true">
						<span class="h-px flex-1 bg-line"></span>
						{m.auth_login_or()}
						<span class="h-px flex-1 bg-line"></span>
					</div>
				{/if}

				<Field label={m.auth_login_username()} for="invite-username" error={errors.username} help={m.w6accounts_invite_username_help()}>
					<input
						id="invite-username"
						type="text"
						class="input"
						bind:value={username}
						autocomplete="username"
						autocapitalize="off"
						spellcheck="false"
						maxlength="64"
						disabled={sending}
						aria-invalid={errors.username ? 'true' : undefined}
						oninput={() => (errors = { ...errors, username: undefined })}
					/>
				</Field>
				<Field label={m.auth_login_password()} for="invite-password" error={errors.password} help={lengthHelp}>
					<PasswordInput
						id="invite-password"
						bind:value={password}
						autocomplete="new-password"
						disabled={sending}
						invalid={!!errors.password}
						oninput={() => (errors = { ...errors, password: undefined })}
					/>
					<PasswordStrength value={password} />
				</Field>
				<Field label={m.auth_setup_confirm()} for="invite-confirm" error={errors.confirm}>
					<PasswordInput
						id="invite-confirm"
						bind:value={confirmation}
						autocomplete="new-password"
						disabled={sending}
						invalid={!!errors.confirm}
						oninput={() => (errors = { ...errors, confirm: undefined })}
					/>
				</Field>
				{#if failure}
					<ErrorNotice title={m.w6accounts_invite_failed()} error={failure} />
				{/if}
				<ClickSpark class="w-full">
					<Button type="submit" variant="primary" size="lg" class="w-full" loading={sending}>{m.w6accounts_invite_submit()}</Button>
				</ClickSpark>
			</form>
		{/if}
	{/if}
</AccountGate>
