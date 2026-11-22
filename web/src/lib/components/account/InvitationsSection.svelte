<script lang="ts">
	/**
	 * Settings → Team & access → Invitations: invite someone by email with a
	 * role and teams, see who has not answered yet (resend, revoke), and set
	 * how long reset links and invitations last, and whether "Forgot
	 * password?" sends a link. Administrators only.
	 *
	 * Without an email server the link is shown once, to copy and hand over.
	 */
	import { MailPlus, Send } from 'lucide-svelte';
	import { ApiError, type Team } from '#lib/api/index.js';
	import { listTeams } from '#lib/api/teams.js';
	import {
		createInvitation,
		getAccountLinkSettings,
		listInvitations,
		resendInvitation,
		revokeInvitation,
		saveAccountLinkSettings,
		type AccountLinkSettingsView,
		type InvitableRole,
		type Invitation,
		type InvitationSent
	} from '#lib/api/accounts.js';
	import { formatDateTime } from '#lib/format.js';
	import { Button, Confirm, CopyBlock, ErrorNotice, Field, Panel, Plate, Skeleton, Toggle } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	let invitations = $state<Invitation[]>([]);
	let teams = $state<Team[]>([]);
	let settings = $state<AccountLinkSettingsView | null>(null);
	let loading = $state(true);
	let loadError = $state<unknown>(null);

	async function load(signal?: AbortSignal) {
		loading = true;
		loadError = null;
		try {
			const [list, links, teamList] = await Promise.all([
				listInvitations(signal),
				getAccountLinkSettings(signal),
				listTeams(signal).catch(() => null)
			]);
			invitations = list;
			settings = links;
			teams = teamList?.teams ?? [];
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			loadError = cause;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	const ROLE_LABELS: Record<InvitableRole, () => string> = {
		admin: () => m.settings_users_role_admin(),
		operator: () => m.settings_users_role_operator(),
		viewer: () => m.settings_users_role_viewer()
	};

	// --- Link settings ----------------------------------------------------------

	const RESET_CHOICES = [15, 30, 60, 120, 1440];
	const INVITE_CHOICES = [24, 72, 168, 336, 720];
	const withCurrent = (choices: number[], current: number | undefined) =>
		current !== undefined && !choices.includes(current) ? [...choices, current].sort((a, b) => a - b) : choices;
	let settingsSaving = $state(false);
	let settingsError = $state<unknown>(null);

	function minutesLabel(minutes: number): string {
		if (minutes % 1440 === 0) return m.w6accounts_invites_days({ count: minutes / 1440 });
		if (minutes % 60 === 0) return m.w6accounts_invites_hours({ count: minutes / 60 });
		return m.w6accounts_invites_minutes({ count: minutes });
	}
	function hoursLabel(hours: number): string {
		return hours >= 24 && hours % 24 === 0 ? m.w6accounts_invites_days({ count: hours / 24 }) : m.w6accounts_invites_hours({ count: hours });
	}

	async function saveSettings(change: Partial<AccountLinkSettingsView>) {
		if (!settings) return;
		settingsSaving = true;
		settingsError = null;
		const { self_service, reset_minutes, invite_hours } = { ...settings, ...change };
		try {
			settings = await saveAccountLinkSettings({ self_service, reset_minutes, invite_hours });
		} catch (cause) {
			settingsError = cause;
		} finally {
			settingsSaving = false;
		}
	}

	const selfServiceNote = $derived.by(() => {
		if (!settings || !settings.self_service) return m.w6accounts_invites_self_service_off();
		if (!settings.mail_server) return m.w6accounts_invites_self_service_no_mail();
		if (!settings.link_base) return m.w6accounts_invites_self_service_no_base();
		return m.w6accounts_invites_self_service_on({ base: settings.link_base });
	});

	// --- Invite -----------------------------------------------------------------

	let adding = $state(false);
	let draft = $state({ email: '', role: 'viewer' as InvitableRole, teams: [] as number[], send: true });
	let emailError = $state<string | null>(null);
	let creating = $state(false);
	let createError = $state<unknown>(null);
	/** The last link, shown once. */
	let shown = $state<InvitationSent | null>(null);

	function openAdd() {
		draft = { email: '', role: 'viewer', teams: [], send: settings?.mail_server ?? false };
		emailError = null;
		createError = null;
		adding = true;
	}

	function toggleTeam(id: number) {
		draft.teams = draft.teams.includes(id) ? draft.teams.filter((t) => t !== id) : [...draft.teams, id];
	}

	async function submitAdd(event: SubmitEvent) {
		event.preventDefault();
		createError = null;
		const email = draft.email.trim();
		emailError = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? null : m.settings_users_err_email();
		if (emailError) return;
		creating = true;
		try {
			const sent = await createInvitation({ email, role: draft.role, teams: draft.teams, send: draft.send });
			invitations = [sent.invitation, ...invitations.filter((i) => i.email.toLowerCase() !== email.toLowerCase())];
			shown = sent;
			adding = false;
		} catch (cause) {
			if (cause instanceof ApiError && (cause.status === 409 || (cause.status === 400 && /email/i.test(cause.message)))) {
				emailError = cause.message;
			} else {
				createError = cause;
			}
		} finally {
			creating = false;
		}
	}

	// --- Resend / revoke ----------------------------------------------------------

	let busyId = $state<number | null>(null);
	let rowError = $state<{ id: number; cause: unknown } | null>(null);

	async function resend(invitation: Invitation) {
		busyId = invitation.id;
		rowError = null;
		try {
			const sent = await resendInvitation(invitation.id, settings?.mail_server ?? false);
			invitations = invitations.map((i) => (i.id === invitation.id ? sent.invitation : i));
			shown = sent;
		} catch (cause) {
			rowError = { id: invitation.id, cause };
		} finally {
			busyId = null;
		}
	}

	async function revoke(invitation: Invitation) {
		busyId = invitation.id;
		rowError = null;
		try {
			await revokeInvitation(invitation.id);
			invitations = invitations.filter((i) => i.id !== invitation.id);
			if (shown?.invitation.id === invitation.id) shown = null;
		} catch (cause) {
			rowError = { id: invitation.id, cause };
		} finally {
			busyId = null;
		}
	}
</script>

<Panel id="invitations" title={m.w6accounts_invites_title()} description={m.w6accounts_invites_description()} padded={false}>
	{#snippet aside()}
		{#if !loading && !loadError && !adding}
			<Button variant="secondary" size="sm" onclick={openAdd}>
				<MailPlus class="size-4" aria-hidden="true" />
				{m.w6accounts_invites_add()}
			</Button>
		{/if}
	{/snippet}

	<div class="px-5 py-4">
		{#if loadError}
			<ErrorNotice error={loadError} title={m.w6accounts_invites_load_error()} onretry={() => void load()} />
		{:else if loading}
			<Skeleton class="h-16 w-full" rows={2} />
		{:else}
			{#if adding}
				<form class="rise-in mb-4 rounded-[var(--radius-card)] border border-line-strong bg-surface-2/40 p-4" onsubmit={submitAdd} novalidate aria-label={m.w6accounts_invites_form_aria()}>
					<div class="grid gap-4 sm:grid-cols-2">
						<Field label={m.settings_users_email()} for="invite-email" error={emailError} required>
							<input
								id="invite-email"
								type="email"
								class="input"
								bind:value={draft.email}
								placeholder="jane@example.org"
								autocomplete="off"
								spellcheck="false"
								maxlength="254"
								disabled={creating}
								aria-invalid={emailError ? 'true' : undefined}
								oninput={() => (emailError = null)}
							/>
						</Field>
						<Field label={m.settings_users_role()} for="invite-role">
							<select id="invite-role" class="input min-h-10" bind:value={draft.role} disabled={creating}>
								<option value="viewer">{m.settings_users_role_viewer_option()}</option>
								<option value="operator">{m.settings_users_role_operator_option()}</option>
								<option value="admin">{m.settings_users_role_admin_option()}</option>
							</select>
						</Field>
					</div>
					{#if teams.length > 0}
						<fieldset class="mt-4">
							<legend class="mb-1.5 text-sm font-semibold text-ink">{m.w6accounts_invites_teams()}</legend>
							<div class="flex flex-wrap gap-2">
								{#each teams as team (team.id)}
									<button
										type="button"
										class={`min-h-10 rounded-lg border px-3 py-1.5 text-sm font-medium transition ${draft.teams.includes(team.id) ? 'border-signal bg-signal-soft text-signal-ink' : 'border-line-strong bg-surface text-ink-2 hover:text-ink'}`}
										aria-pressed={draft.teams.includes(team.id)}
										disabled={creating}
										onclick={() => toggleTeam(team.id)}
									>
										{team.name}
									</button>
								{/each}
							</div>
						</fieldset>
					{/if}
					<div class="mt-4">
						<Field label={m.w6accounts_invites_send()} for="invite-send" inline help={settings?.mail_server ? undefined : m.w6accounts_invites_send_no_mail()}>
							<Toggle id="invite-send" checked={draft.send} disabled={creating || !settings?.mail_server} onchange={(value) => (draft.send = value)} />
						</Field>
					</div>
					{#if createError}
						<ErrorNotice error={createError} title={m.w6accounts_invites_create_error()} class="mt-3" />
					{/if}
					<div class="mt-4 flex flex-wrap items-center gap-2">
						<Button type="submit" variant="primary" loading={creating}>{m.w6accounts_invites_create()}</Button>
						<Button variant="ghost" onclick={() => (adding = false)} disabled={creating}>{m.settings_users_cancel()}</Button>
					</div>
				</form>
			{/if}

			<div aria-live="polite">
				{#if shown}
					<div class="rise-in mb-4 rounded-[var(--radius-card)] border border-advisory/40 bg-surface p-4">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="flex min-w-0 flex-wrap items-center gap-2">
								<p class="font-semibold break-all text-ink">
									{shown.emailed ? m.w6accounts_invites_sent({ email: shown.invitation.email }) : m.w6accounts_invites_copy({ email: shown.invitation.email })}
								</p>
								<Plate tone="advisory" label={m.w6accounts_link_shown()} />
							</div>
							<Button variant="ghost" size="sm" onclick={() => (shown = null)}>{m.settings_users_done()}</Button>
						</div>
						{#if shown.email_error}
							<p class="mt-2 text-[0.8125rem] text-warning-ink">{m.w6accounts_invites_email_failed({ error: shown.email_error })}</p>
						{/if}
						<div class="mt-3">
							<CopyBlock value={shown.url} label={m.w6accounts_invites_copy_link()} secret />
						</div>
					</div>
				{/if}
			</div>

			{#if settings}
				<div class="mb-4 grid gap-3 rounded-[var(--radius-card)] border border-line p-4">
					<div class="flex items-start justify-between gap-3">
						<div class="min-w-0">
							<label for="links-self-service" class="text-sm font-semibold text-ink">{m.w6accounts_invites_self_service()}</label>
							<p class="mt-0.5 text-[0.8125rem] break-words text-ink-2" aria-live="polite">{selfServiceNote}</p>
						</div>
						<Toggle id="links-self-service" checked={settings.self_service} disabled={settingsSaving} onchange={(value) => void saveSettings({ self_service: value })} />
					</div>
					<div class="grid gap-3 sm:grid-cols-2">
						<Field label={m.w6accounts_invites_reset_lifetime()} for="links-reset-minutes">
							<select
								id="links-reset-minutes"
								class="input min-h-10"
								value={settings.reset_minutes}
								disabled={settingsSaving}
								onchange={(event) => void saveSettings({ reset_minutes: Number(event.currentTarget.value) })}
							>
								{#each withCurrent(RESET_CHOICES, settings.reset_minutes) as minutes (minutes)}
									<option value={minutes}>{minutesLabel(minutes)}</option>
								{/each}
							</select>
						</Field>
						<Field label={m.w6accounts_invites_invite_lifetime()} for="links-invite-hours">
							<select
								id="links-invite-hours"
								class="input min-h-10"
								value={settings.invite_hours}
								disabled={settingsSaving}
								onchange={(event) => void saveSettings({ invite_hours: Number(event.currentTarget.value) })}
							>
								{#each withCurrent(INVITE_CHOICES, settings.invite_hours) as hours (hours)}
									<option value={hours}>{hoursLabel(hours)}</option>
								{/each}
							</select>
						</Field>
					</div>
					{#if settingsError}
						<ErrorNotice error={settingsError} title={m.w6accounts_invites_settings_error()} />
					{/if}
				</div>
			{/if}

			{#if invitations.length === 0}
				<p class="rounded-[var(--radius-card)] border border-line px-4 py-6 text-center text-sm text-ink-2">{m.w6accounts_invites_empty()}</p>
			{:else}
				<ul class="divide-y divide-line rounded-[var(--radius-card)] border border-line" role="list">
					{#each invitations as invitation (invitation.id)}
						{@const busy = busyId === invitation.id}
						<li class="px-4 py-3">
							<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
								<div class="min-w-0 flex-[1_1_14rem]">
									<div class="flex flex-wrap items-center gap-2">
										<span class="font-semibold break-all text-ink">{invitation.email}</span>
										<Plate tone="info" bare label={ROLE_LABELS[invitation.role]?.() ?? invitation.role} />
										{#if invitation.expired}<Plate tone="muted" label={m.w6accounts_invites_expired()} />{/if}
									</div>
									<p class="mt-1 text-sm text-ink-2">
										<span class="tnum">
											{invitation.expired
												? m.w6accounts_invites_row_expired({ by: invitation.invited_by, when: formatDateTime(invitation.expires_at) })
												: m.w6accounts_invites_row_pending({ by: invitation.invited_by, when: formatDateTime(invitation.expires_at) })}
										</span>
									</p>
									{#if invitation.teams.length > 0}
										<p class="mt-0.5 text-[0.8125rem] text-ink-2">{m.w6accounts_invites_row_teams({ teams: invitation.teams.map((t) => t.name).join(', ') })}</p>
									{/if}
								</div>
								<div class="flex flex-wrap items-center gap-2">
									<Button variant="secondary" size="sm" loading={busy} onclick={() => resend(invitation)}>
										<Send class="size-4" aria-hidden="true" />
										{m.w6accounts_invites_resend()}
									</Button>
									<Confirm confirmLabel={m.w6accounts_invites_revoke_confirm()} loading={busy} onconfirm={() => revoke(invitation)}>{m.w6accounts_invites_revoke()}</Confirm>
								</div>
							</div>
							{#if rowError?.id === invitation.id}
								<ErrorNotice error={rowError.cause} title={m.w6accounts_invites_row_error()} class="mt-3" />
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</div>
</Panel>
