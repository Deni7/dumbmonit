<script lang="ts">
	/**
	 * Settings → Notifications → Email server: one SMTP server for the
	 * instance. It mails status page subscribers, and scheduled reports when no
	 * email channel is enabled. The password is stored encrypted and never
	 * comes back; an empty field keeps it, as long as the host and username
	 * stay the same. When `DUMBMONIT_SMTP_*` set it, the form is read-only.
	 * "Send a test email" tries the form as it is, saved or not.
	 */
	import { Send } from 'lucide-svelte';
	import { getMailServer, saveMailServer, testMailServer, type MailSecurity, type MailServer } from '#lib/api/index.js';
	import { Button, DocLink, ErrorNotice, Field, Panel, Plate, Skeleton, Toggle } from '#lib/ui/index.js';
	import PasswordInput from './PasswordInput.svelte';
	import UnsavedBadge from './UnsavedBadge.svelte';
	import { useDirtyTracker } from '#lib/settings/unsaved.svelte.js';
	import { m } from '#lib/paraglide/messages.js';
	import { defaultPort, formFrom, mailServerPayload, parsePort, storedPasswordApplies, type MailServerForm } from './mailServer';

	let server = $state<MailServer | null>(null);
	let loading = $state(true);
	let error = $state<unknown>(null);

	let form = $state<MailServerForm>({
		host: '',
		port: '',
		security: 'starttls',
		username: '',
		password: '',
		clearPassword: false,
		from_address: '',
		from_name: ''
	});
	let errors = $state<{ host?: string; port?: string; from_address?: string; password?: string }>({});

	// The password is compared as typed or not: its text never goes into the snapshot.
	const tracker = useDirtyTracker(() => ({ ...form, password: form.password !== '' }));

	function fill(next: MailServer) {
		server = next;
		form = formFrom(next);
		tracker.markClean();
	}

	async function load(signal?: AbortSignal) {
		loading = true;
		error = null;
		try {
			fill(await getMailServer(signal));
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	const locked = $derived(server?.locked_by_env ?? false);
	const keepsPassword = $derived(storedPasswordApplies(server, form));
	const SECURITY: { value: MailSecurity; label: () => string }[] = [
		{ value: 'starttls', label: () => m.settings_mail_security_starttls() },
		{ value: 'tls', label: () => m.settings_mail_security_tls() },
		{ value: 'none', label: () => m.settings_mail_security_none() }
	];

	/** Checks the form; an empty host and sender mean "no server". */
	function check(): boolean {
		const found: typeof errors = {};
		const host = form.host.trim();
		const from = form.from_address.trim();
		if (host || from) {
			if (!host) found.host = m.settings_mail_err_host();
			else if (/\s|\/|:\/\//.test(host)) found.host = m.settings_mail_err_host_format();
			if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(from)) found.from_address = m.settings_mail_err_from();
			if (parsePort(form.port) === undefined) found.port = m.settings_mail_err_port();
			// A stored password never follows another host or username: it must be typed again.
			if (server?.has_password && !keepsPassword && !form.clearPassword && form.password === '' && form.username.trim()) {
				found.password = m.settings_mail_err_password();
			}
		}
		errors = found;
		return Object.keys(found).length === 0;
	}

	// --- Save -----------------------------------------------------------------

	let saving = $state(false);
	let saveError = $state<unknown>(null);
	let saved = $state(false);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		saveError = null;
		saved = false;
		if (!check()) return;
		saving = true;
		try {
			fill(await saveMailServer(mailServerPayload(form)));
			saved = true;
		} catch (cause) {
			saveError = cause;
		} finally {
			saving = false;
		}
	}

	// --- Test -----------------------------------------------------------------

	let testTo = $state('');
	let testToError = $state<string | null>(null);
	let testing = $state(false);
	let testError = $state<unknown>(null);
	let testSent = $state<string | null>(null);

	async function sendTest() {
		testError = null;
		testSent = null;
		const to = testTo.trim();
		testToError = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(to) ? null : m.settings_mail_err_test_to();
		if (testToError) return;
		// Locked by the environment: the server tests what is in force.
		if (!locked && !check()) return;
		testing = true;
		try {
			await testMailServer(to, locked ? undefined : mailServerPayload(form));
			testSent = to;
		} catch (cause) {
			testError = cause;
		} finally {
			testing = false;
		}
	}

	const canTest = $derived(locked ? !!server?.configured : form.host.trim() !== '' && form.from_address.trim() !== '');
</script>

<Panel id="email-server" title={m.settings_mail_title()} description={m.settings_mail_description()}>
	{#snippet aside()}
		{#if server}
			{#if server.configured}
				<Plate tone="signal" label={m.settings_mail_on()} />
			{:else}
				<Plate tone="ghost" label={m.settings_mail_off()} />
			{/if}
		{/if}
	{/snippet}

	{#if error}
		<ErrorNotice {error} title={m.settings_mail_load_error()} onretry={() => void load()} />
	{:else if loading || !server}
		<Skeleton class="h-10 w-full" rows={5} />
	{:else}
		<form class="grid gap-4" onsubmit={submit} novalidate>
			{#if locked}
				<div class="flex flex-wrap items-start gap-2 rounded-[var(--radius-card)] border border-line bg-surface-2 p-3 text-sm text-ink-2" role="note">
					<Plate tone="ghost" label={m.settings_mail_env_plate()} />
					<p class="min-w-0 flex-1">{m.settings_mail_env_note()}</p>
				</div>
			{/if}

			<fieldset class="grid gap-4 sm:grid-cols-[minmax(0,1fr)_11rem_7rem]" disabled={locked || saving}>
				<legend class="sr-only">{m.settings_mail_title()}</legend>
				<Field label={m.settings_mail_host()} for="mail-host" error={errors.host} help={m.settings_mail_host_help()}>
					<input id="mail-host" type="text" class="input" bind:value={form.host} placeholder="smtp.example.com" autocomplete="off" autocapitalize="off" spellcheck="false" aria-invalid={errors.host ? 'true' : undefined} oninput={() => (errors = { ...errors, host: undefined })} />
				</Field>
				<Field label={m.settings_mail_security()} for="mail-security">
					<select id="mail-security" class="input" bind:value={form.security}>
						{#each SECURITY as option (option.value)}
							<option value={option.value}>{option.label()}</option>
						{/each}
					</select>
				</Field>
				<Field label={m.settings_mail_port()} for="mail-port" error={errors.port}>
					<input id="mail-port" type="text" inputmode="numeric" class="input tnum" bind:value={form.port} placeholder={String(defaultPort(form.security))} autocomplete="off" aria-invalid={errors.port ? 'true' : undefined} oninput={() => (errors = { ...errors, port: undefined })} />
				</Field>
			</fieldset>
			{#if form.security === 'none' && form.host.trim()}
				<p class="text-[0.8125rem] text-ink-2">{m.settings_mail_security_none_help()}</p>
			{/if}

			<fieldset class="grid gap-4 sm:grid-cols-2" disabled={locked || saving}>
				<legend class="sr-only">{m.settings_mail_auth_legend()}</legend>
				<Field label={m.settings_mail_username()} for="mail-username" help={m.settings_mail_username_help()}>
					<input id="mail-username" type="text" class="input" bind:value={form.username} autocomplete="off" autocapitalize="off" spellcheck="false" />
				</Field>
				<Field label={m.settings_mail_password()} for="mail-password" error={errors.password} help={keepsPassword ? m.settings_mail_password_stored() : m.settings_mail_password_help()}>
					<PasswordInput id="mail-password" bind:value={form.password} autocomplete="new-password" placeholder={keepsPassword ? '••••••••' : ''} disabled={locked || saving || form.clearPassword} invalid={!!errors.password} oninput={() => (errors = { ...errors, password: undefined })} />
				</Field>
				{#if server.has_password && !locked}
					<div class="sm:col-span-2">
						<Field label={m.settings_mail_password_clear()} for="mail-password-clear" inline help={m.settings_mail_password_clear_help()}>
							<Toggle id="mail-password-clear" bind:checked={form.clearPassword} onchange={() => (errors = { ...errors, password: undefined })} />
						</Field>
					</div>
				{/if}
			</fieldset>

			<fieldset class="grid gap-4 sm:grid-cols-2" disabled={locked || saving}>
				<legend class="sr-only">{m.settings_mail_sender_legend()}</legend>
				<Field label={m.settings_mail_from()} for="mail-from" error={errors.from_address}>
					<input id="mail-from" type="email" class="input" bind:value={form.from_address} placeholder="status@example.com" autocomplete="off" spellcheck="false" aria-invalid={errors.from_address ? 'true' : undefined} oninput={() => (errors = { ...errors, from_address: undefined })} />
				</Field>
				<Field label={m.settings_mail_from_name()} for="mail-from-name" help={m.settings_mail_from_name_help()}>
					<input id="mail-from-name" type="text" class="input" bind:value={form.from_name} maxlength="120" placeholder="Example status" autocomplete="off" />
				</Field>
			</fieldset>

			{#if saveError}
				<ErrorNotice error={saveError} title={m.settings_mail_save_error()} />
			{/if}

			{#if !locked}
				<div class="flex flex-wrap items-center gap-2" aria-live="polite">
					<Button type="submit" variant="secondary" loading={saving} success={saved}>{m.settings_mail_save()}</Button>
					<UnsavedBadge dirty={tracker.dirty} />
					{#if saved}
						<Plate tone="signal" label={m.settings_mail_saved()} draw />
					{/if}
				</div>
			{/if}
		</form>

		<div class="mt-6 grid gap-3 border-t border-line pt-4">
			<Field label={m.settings_mail_test_to()} for="mail-test-to" error={testToError} help={m.settings_mail_test_help()}>
				<div class="flex flex-wrap items-center gap-2">
					<input id="mail-test-to" type="email" class="input min-w-0 flex-[1_1_14rem]" bind:value={testTo} placeholder="you@example.com" autocomplete="email" spellcheck="false" disabled={testing} oninput={() => (testToError = null)} />
					<Button variant="secondary" class="shrink-0 max-sm:w-full" onclick={() => void sendTest()} loading={testing} disabled={saving || !canTest}>
						<Send class="size-4 shrink-0" aria-hidden="true" />
						<span>{m.settings_mail_test()}</span>
					</Button>
				</div>
			</Field>
			<div aria-live="polite" class="empty:hidden">
				{#if testError}
					<ErrorNotice error={testError} title={m.settings_mail_test_error()} hint={m.settings_mail_test_error_hint()} />
				{:else if testSent}
					<p class="flex flex-wrap items-center gap-2 text-sm text-ink-2">
						<Plate tone="signal" label={m.settings_mail_test_sent_plate()} draw />
						<span class="min-w-0">{m.settings_mail_test_sent({ to: testSent })}</span>
					</p>
				{/if}
			</div>
			<DocLink page="notifications/#email-server" />
		</div>
	{/if}
</Panel>
