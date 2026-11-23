<script lang="ts">
	/**
	 * Settings → Users: the accounts that can sign in, and their role.
	 *
	 * Three roles: admin (everything), operator (handles alerts — ack, ignore,
	 * snooze — but changes no configuration) and viewer (read only), plus wall
	 * display (signs in straight to the wall, reaches nothing else). A custom
	 * role (Custom roles, below) can replace the built-in one. The server keeps
	 * at least one active admin; the disabled controls here just reflect that
	 * rule so nobody discovers it through an error.
	 *
	 * Above the list: the instance policy "require two-factor for admins and
	 * operators", then search, a "without 2FA" filter and the sort order.
	 */
	import { UserPlus, Users, RefreshCw, Search } from 'lucide-svelte';
	import {
		ApiError,
		createUser,
		deleteUser,
		listUsers,
		updateUser,
		type CreateUserPayload,
		type Role,
		type User
	} from '#lib/api/index.js';
	import { formatDateTime, formatRelative } from '#lib/format.js';
	import { auth, PASSWORD_MIN_LENGTH, validatePassword } from '#lib/stores/auth.svelte.js';
	import { Button, Confirm, CopyBlock, EmptyState, ErrorNotice, Field, Panel, Plate, Skeleton, Toggle } from '#lib/ui/index.js';
	import PasswordInput from './PasswordInput.svelte';
	import { resetUserTotp } from '#lib/api/totp.js';
	import { getAuthPolicy, saveAuthPolicy, signOutUser } from '#lib/api/sessions.js';
	import { m } from '#lib/paraglide/messages.js';
	import { USER_SORTS, filterUsers, sortUsers, type UserSort } from '#lib/components/settings/usersList.js';
	import ResetLinkButton from '#lib/components/account/ResetLinkButton.svelte';
	import ResetLinkNotice from '#lib/components/account/ResetLinkNotice.svelte';
	import type { AccountLink } from '#lib/api/accounts.js';
	import type { CustomRole } from '#lib/api/types.js';
	import { listCustomRoles } from '#lib/api/roles.js';
	import { choiceOf, isCustomChoice, rolePayload, type RoleChoice } from './roles/roleChoice.js';

	let users = $state<User[]>([]);
	let loading = $state(true);
	let error = $state<unknown>(null);

	async function load(signal?: AbortSignal) {
		loading = true;
		error = null;
		try {
			const list = await listUsers(signal);
			users = Array.isArray(list) ? list : [];
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		} finally {
			loading = false;
		}
	}

	/** Custom roles offered in the role picker; none if they cannot be read. */
	let customRoles = $state<CustomRole[]>([]);
	async function loadRoles(signal?: AbortSignal) {
		try {
			const list = await listCustomRoles(signal);
			customRoles = Array.isArray(list) ? list : [];
		} catch {
			customRoles = [];
		}
	}

	$effect(() => {
		const controller = new AbortController();
		void load(controller.signal);
		void loadPolicy(controller.signal);
		void loadRoles(controller.signal);
		return () => controller.abort();
	});

	// --- Search, filter, sort -------------------------------------------------

	let query = $state('');
	let without2fa = $state(false);
	let sort = $state<UserSort>('name');
	const SORT_LABELS: Record<UserSort, () => string> = {
		name: () => m.settings_users_sort_name(),
		role: () => m.settings_users_sort_role(),
		last_login: () => m.settings_users_sort_last_login()
	};
	const shown = $derived(sortUsers(filterUsers(users, query, without2fa), sort));

	// --- Instance policy: two-factor for admins and operators ----------------

	let requireTotp = $state(false);
	let policyLoaded = $state(false);
	let policySaving = $state(false);
	let policyError = $state<unknown>(null);

	async function loadPolicy(signal?: AbortSignal) {
		try {
			requireTotp = (await getAuthPolicy(signal)).require_totp;
			policyLoaded = true;
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			policyError = cause;
		}
	}

	async function setRequireTotp(value: boolean) {
		policySaving = true;
		policyError = null;
		try {
			requireTotp = (await saveAuthPolicy({ require_totp: value })).require_totp;
			// Turning it on may hold this very session at enrolment.
			void auth.refresh();
		} catch (cause) {
			requireTotp = !value;
			policyError = cause;
		} finally {
			policySaving = false;
		}
	}

	/** Staff accounts the policy would hold at enrolment (local password, no second factor). Viewers and wall displays are never held. */
	const staffWithout2fa = $derived(
		users.filter((u) => (u.role === 'admin' || u.role === 'operator') && !u.disabled && !u.totp_enabled && u.auth === 'password').length
	);

	const activeAdmins = $derived(users.filter((u) => u.role === 'admin' && !u.disabled).length);
	/** True when this user is the only active admin: the server would refuse to demote, disable or delete it. */
	function isLastAdmin(user: User): boolean {
		return user.role === 'admin' && !user.disabled && activeAdmins <= 1;
	}
	function isMe(user: User): boolean {
		return auth.user?.id === user.id;
	}

	/** A random passphrase-like password, long enough for the server's rule. */
	function generatePassword(): string {
		const alphabet = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
		const bytes = new Uint8Array(20);
		crypto.getRandomValues(bytes);
		return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join('');
	}

	// --- Add ------------------------------------------------------------------

	let adding = $state(false);
	const ROLE_LABELS: Record<Role, () => string> = {
		admin: () => m.settings_users_role_admin(),
		operator: () => m.settings_users_role_operator(),
		viewer: () => m.settings_users_role_viewer(),
		wall: () => m.settings_users_role_wall()
	};
	const ROLE_TONES: Record<Role, 'signal' | 'info' | 'ghost'> = { admin: 'signal', operator: 'info', viewer: 'ghost', wall: 'ghost' };

	function roleHelp(choice: RoleChoice): string | undefined {
		if (choice === 'wall') return m.settings_users_role_wall_help();
		if (isCustomChoice(choice)) return m.w6roles_users_custom_help();
		return undefined;
	}

	let draft = $state({ username: '', display_name: '', email: '', role: 'viewer' as RoleChoice, password: '' });
	let draftErrors = $state<{ username?: string; password?: string; email?: string }>({});
	let creating = $state(false);
	let createError = $state<unknown>(null);
	/** The password shown once, right after creation, so the admin can hand it over. */
	let created = $state<{ user: User; password: string } | null>(null);

	function openAdd() {
		draft = { username: '', display_name: '', email: '', role: 'viewer', password: '' };
		draftErrors = {};
		createError = null;
		adding = true;
	}

	async function submitAdd(event: SubmitEvent) {
		event.preventDefault();
		createError = null;
		const found: typeof draftErrors = {};
		const username = draft.username.trim();
		if (!username) found.username = m.settings_users_err_username();
		else if (/\s/.test(username)) found.username = m.settings_users_err_spaces();
		const password = draft.password;
		if (password) {
			const problem = validatePassword(password);
			if (problem) found.password = problem;
		} else if (!auth.oidc.enabled && !auth.ldap.enabled) {
			found.password = m.settings_users_err_password();
		}
		const emailProblem = checkEmail(draft.email);
		if (emailProblem) found.email = emailProblem;
		draftErrors = found;
		if (Object.keys(found).length > 0) return;

		creating = true;
		try {
			const payload: CreateUserPayload = { username, ...rolePayload(draft.role) };
			if (draft.display_name.trim()) payload.display_name = draft.display_name.trim();
			if (draft.email.trim()) payload.email = draft.email.trim();
			if (password) payload.password = password;
			const user = await createUser(payload);
			users = [...users, user].sort((a, b) => a.username.localeCompare(b.username));
			created = password ? { user, password } : null;
			adding = false;
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 409) {
				draftErrors = { username: cause.message };
			} else if (cause instanceof ApiError && cause.status === 400 && /email/i.test(cause.message)) {
				draftErrors = { email: cause.message };
			} else {
				createError = cause;
			}
		} finally {
			creating = false;
		}
	}

	// --- Edit -----------------------------------------------------------------

	/**
	 * Light check before sending: something@domain.tld, no space. The server
	 * has the final word and answers 400 with the reason.
	 */
	function checkEmail(value: string): string | null {
		const email = value.trim();
		if (!email) return null;
		return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? null : m.settings_users_err_email();
	}

	let editingId = $state<number | null>(null);
	let edit = $state({ display_name: '', email: '', role: 'viewer' as RoleChoice, password: '' });
	let editError = $state<unknown>(null);
	let editPasswordError = $state<string | null>(null);
	let editEmailError = $state<string | null>(null);
	let saving = $state(false);
	/** Password reset shown once, like on creation. */
	let resetShown = $state<{ user: User; password: string } | null>(null);
	/** Reset link just created from a row: sent, or shown once to copy. */
	let linkShown = $state<{ user: User; link: AccountLink } | null>(null);

	function openEdit(user: User) {
		editingId = user.id;
		edit = { display_name: user.display_name, email: user.email ?? '', role: choiceOf(user), password: '' };
		editError = null;
		editPasswordError = null;
		editEmailError = null;
	}

	async function submitEdit(event: SubmitEvent, user: User) {
		event.preventDefault();
		editError = null;
		editPasswordError = edit.password ? validatePassword(edit.password) : null;
		editEmailError = checkEmail(edit.email);
		if (editPasswordError || editEmailError) return;

		saving = true;
		try {
			const updated = await updateUser(user.id, {
				display_name: edit.display_name.trim(),
				email: edit.email.trim(),
				...rolePayload(edit.role),
				...(edit.password ? { password: edit.password } : {})
			});
			users = users.map((u) => (u.id === updated.id ? updated : u));
			resetShown = edit.password ? { user: updated, password: edit.password } : null;
			editingId = null;
			// Changing one's own role or name is visible in the top bar right away.
			if (isMe(user)) void auth.refresh();
		} catch (cause) {
			editError = cause;
		} finally {
			saving = false;
		}
	}

	// --- Disable / delete -------------------------------------------------------

	let busyId = $state<number | null>(null);
	let rowError = $state<{ id: number; cause: unknown } | null>(null);

	async function setDisabled(user: User, disabled: boolean) {
		busyId = user.id;
		rowError = null;
		try {
			const updated = await updateUser(user.id, { disabled });
			users = users.map((u) => (u.id === updated.id ? updated : u));
		} catch (cause) {
			rowError = { id: user.id, cause };
		} finally {
			busyId = null;
		}
	}

	/** Admin: removes someone's second factor (lost phone, no recovery code). */
	async function resetTotp(user: User) {
		busyId = user.id;
		rowError = null;
		try {
			await resetUserTotp(user.id);
			users = users.map((u) => (u.id === user.id ? { ...u, totp_enabled: false } : u));
		} catch (cause) {
			rowError = { id: user.id, cause };
		} finally {
			busyId = null;
		}
	}

	/** Users whose sessions were just closed, to confirm it on their row. */
	let signedOut = $state<{ id: number; count: number } | null>(null);

	/** Admin: closes every session of the account (for oneself, this one stays). */
	async function signOutEverywhere(user: User) {
		busyId = user.id;
		rowError = null;
		signedOut = null;
		try {
			signedOut = { id: user.id, count: await signOutUser(user.id) };
		} catch (cause) {
			rowError = { id: user.id, cause };
		} finally {
			busyId = null;
		}
	}

	async function remove(user: User) {
		busyId = user.id;
		rowError = null;
		try {
			await deleteUser(user.id);
			users = users.filter((u) => u.id !== user.id);
			if (created?.user.id === user.id) created = null;
		} catch (cause) {
			rowError = { id: user.id, cause };
		} finally {
			busyId = null;
		}
	}
</script>

{#snippet roleOptions()}
	<option value="viewer">{m.settings_users_role_viewer_option()}</option>
	<option value="operator">{m.settings_users_role_operator_option()}</option>
	<option value="admin">{m.settings_users_role_admin_option()}</option>
	<option value="wall">{m.settings_users_role_wall_option()}</option>
	{#if customRoles.length > 0}
		<optgroup label={m.w6roles_users_custom_group()}>
			{#each customRoles as role (role.id)}
				<option value={`custom:${role.id}`}>{role.name}</option>
			{/each}
		</optgroup>
	{/if}
{/snippet}

<Panel id="users" title={m.settings_users_title()} description={m.settings_users_description()} padded={false}>
	{#snippet aside()}
		{#if !loading && !error && !adding}
			<Button variant="secondary" size="sm" onclick={openAdd}>
				<UserPlus class="size-4" aria-hidden="true" />
				{m.settings_users_add()}
			</Button>
		{/if}
	{/snippet}

	<div class="px-5 py-4">
		{#if adding}
			<form class="rise-in mb-4 rounded-[var(--radius-card)] border border-line-strong bg-surface-2/40 p-4" onsubmit={submitAdd} novalidate aria-label={m.settings_users_new_aria()}>
				<div class="grid gap-4 sm:grid-cols-2">
					<Field label={m.settings_users_username()} for="new-username" error={draftErrors.username} required>
						<input
							id="new-username"
							type="text"
							class="input"
							bind:value={draft.username}
							autocomplete="off"
							autocapitalize="off"
							spellcheck="false"
							placeholder="jane"
							disabled={creating}
							aria-invalid={draftErrors.username ? 'true' : undefined}
							oninput={() => (draftErrors = { ...draftErrors, username: undefined })}
						/>
					</Field>
					<Field label={m.settings_users_display_name()} for="new-display-name" help={m.settings_users_display_help()}>
						<input id="new-display-name" type="text" class="input" bind:value={draft.display_name} placeholder="Jane Doe" autocomplete="off" disabled={creating} />
					</Field>
					<Field label={m.settings_users_email()} for="new-email" error={draftErrors.email} help={m.settings_users_email_help()}>
						<input
							id="new-email"
							type="email"
							class="input"
							bind:value={draft.email}
							placeholder="jane@example.org"
							autocomplete="off"
							spellcheck="false"
							maxlength="254"
							disabled={creating}
							aria-invalid={draftErrors.email ? 'true' : undefined}
							oninput={() => (draftErrors = { ...draftErrors, email: undefined })}
						/>
					</Field>
					<Field label={m.settings_users_role()} for="new-role" help={roleHelp(draft.role)}>
						<select id="new-role" class="input min-h-10" bind:value={draft.role} disabled={creating}>
							{@render roleOptions()}
						</select>
					</Field>
					<Field
						label={m.settings_users_password()}
						for="new-password"
						error={draftErrors.password}
						help={auth.oidc.enabled
							? m.settings_users_password_help_sso({ min: PASSWORD_MIN_LENGTH, provider: auth.oidc.provider_name })
							: auth.ldap.enabled
								? m.settings_users_password_help_ldap({ min: PASSWORD_MIN_LENGTH })
								: m.settings_users_password_help({ min: PASSWORD_MIN_LENGTH })}
					>
						<div class="flex gap-2">
							<PasswordInput
								id="new-password"
								class="min-w-0 flex-1"
								bind:value={draft.password}
								autocomplete="new-password"
								disabled={creating}
								invalid={!!draftErrors.password}
								oninput={() => (draftErrors = { ...draftErrors, password: undefined })}
							/>
							<Button variant="ghost" onclick={() => (draft.password = generatePassword())} disabled={creating} title={m.settings_users_generate_title()}>
								<RefreshCw class="size-4" aria-hidden="true" />
								{m.settings_users_generate()}
							</Button>
						</div>
					</Field>
				</div>
				{#if createError}
					<ErrorNotice error={createError} title={m.settings_users_create_error()} class="mt-3" />
				{/if}
				<div class="mt-4 flex flex-wrap items-center gap-2">
					<Button type="submit" variant="primary" loading={creating}>{m.settings_users_create()}</Button>
					<Button variant="ghost" onclick={() => (adding = false)} disabled={creating}>{m.settings_users_cancel()}</Button>
				</div>
			</form>
		{/if}

		<div aria-live="polite">
			{#if created}
				<div class="rise-in mb-4 rounded-[var(--radius-card)] border border-advisory/40 bg-surface p-4">
					<div class="flex flex-wrap items-center justify-between gap-2">
						<div class="flex flex-wrap items-center gap-2">
							<p class="font-semibold text-ink">{m.settings_users_created({ username: created.user.username })}</p>
							<Plate tone="advisory" label={m.settings_users_created_shown()} />
						</div>
						<Button variant="ghost" size="sm" onclick={() => (created = null)}>{m.settings_users_done()}</Button>
					</div>
					<div class="mt-3">
						<CopyBlock value={created.password} label={m.settings_users_copy_password()} secret />
					</div>
				</div>
			{/if}
			{#if linkShown}
				<ResetLinkNotice user={linkShown.user} link={linkShown.link} ondone={() => (linkShown = null)} />
			{/if}
			{#if resetShown}
				<div class="rise-in mb-4 rounded-[var(--radius-card)] border border-advisory/40 bg-surface p-4">
					<div class="flex flex-wrap items-center justify-between gap-2">
						<div class="flex flex-wrap items-center gap-2">
							<p class="font-semibold text-ink">{m.settings_users_reset_title({ username: resetShown.user.username })}</p>
							<Plate tone="advisory" label={m.settings_users_reset_shown()} />
						</div>
						<Button variant="ghost" size="sm" onclick={() => (resetShown = null)}>{m.settings_users_done()}</Button>
					</div>
					<div class="mt-3">
						<CopyBlock value={resetShown.password} label={m.settings_users_copy_password()} secret />
					</div>
				</div>
			{/if}
		</div>

		{#if policyLoaded || policyError}
			<div class="mb-4 rounded-[var(--radius-card)] border border-line p-4" data-testid="totp-policy">
				<div class="flex items-start justify-between gap-3">
					<div class="min-w-0">
						<label for="policy-require-totp" class="text-sm font-semibold text-ink">{m.settings_users_policy_title()}</label>
						<p class="mt-0.5 text-[0.8125rem] text-ink-2">{m.settings_users_policy_help()}</p>
					</div>
					<Toggle id="policy-require-totp" checked={requireTotp} disabled={policySaving || !policyLoaded} onchange={(value) => void setRequireTotp(value)} />
				</div>
				{#if requireTotp && staffWithout2fa > 0}
					<p class="mt-2 text-[0.8125rem] text-ink-2" aria-live="polite">{m.settings_users_policy_pending({ count: staffWithout2fa })}</p>
				{/if}
				{#if policyError}
					<ErrorNotice error={policyError} title={m.settings_users_policy_error()} class="mt-3" />
				{/if}
			</div>
		{/if}

		{#if !error && !loading && users.length > 0}
			<div class="mb-3 flex flex-wrap items-end gap-3">
				<div class="relative min-w-0 flex-[1_1_14rem]">
					<label for="users-search" class="sr-only">{m.settings_users_search_label()}</label>
					<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
					<input id="users-search" type="search" class="input pl-9" bind:value={query} placeholder={m.settings_users_search_placeholder()} autocomplete="off" spellcheck="false" />
				</div>
				<div class="inline-flex h-10 items-center gap-2 rounded-lg border border-line px-3">
					<Toggle id="users-without-2fa" checked={without2fa} onchange={(value) => (without2fa = value)} />
					<label for="users-without-2fa" class="text-[0.8125rem] font-semibold whitespace-nowrap text-ink">{m.settings_users_filter_without_2fa()}</label>
				</div>
				<div class="flex items-center gap-2">
					<label for="users-sort" class="text-[0.8125rem] font-semibold whitespace-nowrap text-ink-2">{m.settings_users_sort_label()}</label>
					<select id="users-sort" class="input w-auto" bind:value={sort}>
						{#each USER_SORTS as key (key)}
							<option value={key}>{SORT_LABELS[key]()}</option>
						{/each}
					</select>
				</div>
			</div>
		{/if}

		{#if error}
			<ErrorNotice {error} title={m.settings_users_load_error()} onretry={() => void load()} />
		{:else if loading}
			<Skeleton class="h-16 w-full" rows={2} />
		{:else if users.length === 0}
			<EmptyState icon={Users} title={m.settings_users_empty_title()} description={m.settings_users_empty_description()} />
		{:else if shown.length === 0}
			<p class="rounded-[var(--radius-card)] border border-line px-4 py-6 text-center text-sm text-ink-2" aria-live="polite">{m.settings_users_no_match()}</p>
		{:else}
			<ul class="divide-y divide-line rounded-[var(--radius-card)] border border-line" role="list">
				{#each shown as user (user.id)}
					{@const lastAdmin = isLastAdmin(user)}
					{@const me = isMe(user)}
					{@const busy = busyId === user.id}
					{@const editing = editingId === user.id}
					<li class={`px-4 py-3 ${user.disabled ? 'ghost-cell' : ''}`}>
						<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
							<div class="min-w-0 flex-[1_1_14rem]">
								<div class="flex flex-wrap items-center gap-2">
									<span class={`font-semibold ${user.disabled ? 'text-ink-2' : 'text-ink'}`}>{user.display_name.trim() || user.username}</span>
									{#if user.display_name.trim()}
										<span class="text-sm text-ink-2">{user.username}</span>
									{/if}
									{#if user.custom_role}
										<Plate tone="info" bare label={user.custom_role.name} title={m.w6roles_users_custom_title()} />
									{:else}
										<Plate tone={ROLE_TONES[user.role] ?? 'ghost'} bare label={ROLE_LABELS[user.role]?.() ?? user.role} />
									{/if}
									<Plate
										tone="info"
										bare
										label={user.auth === 'saml' ? auth.saml.label || 'SAML' : user.auth === 'oidc' ? auth.oidc.provider_name || 'SSO' : user.auth === 'ldap' ? 'LDAP' : m.settings_users_auth_password()}
										title={user.auth === 'oidc' || user.auth === 'saml' ? m.settings_users_auth_sso_title() : user.auth === 'ldap' ? m.settings_users_auth_ldap_title() : m.settings_users_auth_password_title()}
									/>
									{#if user.totp_enabled}<Plate tone="signal" bare label="2FA" title={m.settings_users_2fa_title()} />{/if}
									{#if user.disabled}<Plate tone="muted" label={m.settings_users_disabled()} />{/if}
									{#if me}<span class="text-[0.75rem] font-semibold text-ink-3">{m.settings_users_you()}</span>{/if}
								</div>
								{#if user.email}
									<p class="mt-0.5 text-sm break-all text-ink-2">{user.email}</p>
								{/if}
								<p class="mt-1 text-sm text-ink-2">
									<span class="tnum" title={`${formatDateTime(user.last_login_at)} · ${formatDateTime(user.created_at)}`}>{m.settings_users_last_seen({
										last: user.last_login_at ? formatRelative(user.last_login_at) : m.settings_users_never(),
										created: formatRelative(user.created_at)
									})}</span>
								</p>
							</div>
							{#if !editing}
								<div class="flex flex-wrap items-center gap-2">
									<Button variant="ghost" size="sm" disabled={busy} onclick={() => openEdit(user)} aria-label={m.settings_users_edit_aria({ username: user.username })}>{m.settings_users_edit()}</Button>
									<div
										class="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-2.5"
										title={lastAdmin ? m.settings_users_no_disable_last() : me ? m.settings_users_no_disable_self() : undefined}
									>
										<Toggle id={`user-enabled-${user.id}`} checked={!user.disabled} disabled={busy || lastAdmin || me} onchange={(v) => void setDisabled(user, !v)} />
										<label for={`user-enabled-${user.id}`} class="text-[0.8125rem] font-semibold text-ink">{m.settings_users_enabled()}</label>
									</div>
									{#if !user.disabled}
										<span title={me ? m.settings_users_sign_out_self_title() : m.settings_users_sign_out_title()}>
											<Confirm confirmLabel={m.settings_users_sign_out_confirm()} variant="secondary" loading={busy} onconfirm={() => signOutEverywhere(user)}>{m.settings_users_sign_out()}</Confirm>
										</span>
									{/if}
									{#if user.auth === 'password' && user.role !== 'wall' && !user.disabled && !me}
										<ResetLinkButton
											{user}
											disabled={busy}
											onlink={(owner, link) => (linkShown = { user: owner, link })}
											onerror={(cause) => (rowError = { id: user.id, cause })}
										/>
									{/if}
									{#if user.totp_enabled && !me}
										<span title={m.settings_users_reset2fa_title()}>
											<Confirm confirmLabel={m.settings_users_reset2fa_confirm()} variant="secondary" loading={busy} onconfirm={() => resetTotp(user)}>{m.settings_users_reset2fa()}</Confirm>
										</span>
									{/if}
									<span title={lastAdmin ? m.settings_users_no_delete_last() : me ? m.settings_users_no_delete_self() : undefined}>
										<Confirm confirmLabel={m.settings_users_delete_confirm()} loading={busy} disabled={lastAdmin || me} onconfirm={() => remove(user)}>{m.settings_users_delete()}</Confirm>
									</span>
								</div>
							{/if}
						</div>

						{#if editing}
							<form class="rise-in mt-3 grid gap-4 rounded-[var(--radius-card)] border border-line-strong bg-surface-2/40 p-4 sm:grid-cols-2" onsubmit={(e) => submitEdit(e, user)} novalidate aria-label={m.settings_users_edit_aria({ username: user.username })}>
								<Field label={m.settings_users_display_name()} for={`edit-name-${user.id}`}>
									<input id={`edit-name-${user.id}`} type="text" class="input" bind:value={edit.display_name} autocomplete="off" disabled={saving} />
								</Field>
								<Field label={m.settings_users_email()} for={`edit-email-${user.id}`} error={editEmailError} help={m.settings_users_email_help()}>
									<input
										id={`edit-email-${user.id}`}
										type="email"
										class="input"
										bind:value={edit.email}
										placeholder="jane@example.org"
										autocomplete="off"
										spellcheck="false"
										maxlength="254"
										disabled={saving}
										aria-invalid={editEmailError ? 'true' : undefined}
										oninput={() => (editEmailError = null)}
									/>
								</Field>
								<Field
									label={m.settings_users_role()}
									for={`edit-role-${user.id}`}
									help={lastAdmin ? m.settings_users_role_last_admin_help() : roleHelp(edit.role)}
								>
									<select id={`edit-role-${user.id}`} class="input min-h-10" bind:value={edit.role} disabled={saving || lastAdmin} title={lastAdmin ? m.settings_users_promote_first() : undefined}>
										{@render roleOptions()}
									</select>
								</Field>
								<Field
									label={m.settings_users_reset_password()}
									for={`edit-password-${user.id}`}
									error={editPasswordError}
									help={user.auth === 'oidc' ? m.settings_users_reset_help_sso() : user.auth === 'ldap' ? m.settings_users_reset_help_ldap() : m.settings_users_reset_help()}
									class="sm:col-span-2"
								>
									<div class="flex gap-2">
										<PasswordInput id={`edit-password-${user.id}`} class="min-w-0 flex-1" bind:value={edit.password} autocomplete="new-password" disabled={saving} invalid={!!editPasswordError} oninput={() => (editPasswordError = null)} />
										<Button variant="ghost" onclick={() => (edit.password = generatePassword())} disabled={saving} title={m.settings_users_generate_title()}>
											<RefreshCw class="size-4" aria-hidden="true" />
											{m.settings_users_generate()}
										</Button>
									</div>
								</Field>
								{#if editError}
									<div class="sm:col-span-2"><ErrorNotice error={editError} title={m.settings_users_save_error()} /></div>
								{/if}
								<div class="flex flex-wrap items-center gap-2 sm:col-span-2">
									<Button type="submit" variant="secondary" loading={saving}>{m.settings_users_save()}</Button>
									<Button variant="ghost" onclick={() => (editingId = null)} disabled={saving}>{m.settings_users_cancel()}</Button>
								</div>
							</form>
						{/if}

						{#if signedOut?.id === user.id}
							<div class="mt-2" aria-live="polite">
								<Plate tone="signal" draw label={m.settings_users_signed_out({ count: signedOut.count })} />
							</div>
						{/if}
						{#if rowError?.id === user.id}
							<ErrorNotice error={rowError.cause} title={m.settings_users_update_error()} class="mt-3" />
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</Panel>
