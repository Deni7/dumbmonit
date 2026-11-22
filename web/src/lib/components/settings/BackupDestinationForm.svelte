<script lang="ts">
	/**
	 * Add or edit an off-site backup destination: S3-compatible, WebDAV or SFTP.
	 *
	 * Secrets are never read back: on edit they show empty ("saved, leave empty
	 * to keep") and are sent only when typed again. Typing one replaces the
	 * saved set, so the server never mixes an old key with a new account.
	 */
	import { untrack } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import {
		createBackupDestination,
		updateBackupDestination,
		type BackupDestination,
		type BackupDestinationKind,
		type BackupDestinationPayload
	} from '#lib/api/backupDestinations.js';
	import { Button, ErrorNotice, Field, Segmented, Toggle } from '#lib/ui/index.js';
	import PasswordInput from './PasswordInput.svelte';

	interface Props {
		/** Present when editing, absent when adding. */
		destination?: BackupDestination;
		onsaved: (destination: BackupDestination) => void;
		oncancel: () => void;
	}

	let { destination, onsaved, oncancel }: Props = $props();
	const editing = $derived(destination !== undefined);

	const KINDS: BackupDestinationKind[] = ['s3', 'webdav', 'sftp'];
	const kindOptions = [
		{ value: 's3', label: m.w5backups_kind_s3() },
		{ value: 'webdav', label: m.w5backups_kind_webdav() },
		{ value: 'sftp', label: m.w5backups_kind_sftp() }
	];

	// Frozen when the form opens: it must not follow list refreshes while typing.
	const initial = untrack(() => destination);
	const saved = (key: string): string => {
		const value = initial?.settings?.[key];
		return typeof value === 'string' ? value : '';
	};

	let kind = $state<string>(initial && KINDS.includes(initial.kind as BackupDestinationKind) ? initial.kind : 's3');
	let name = $state(initial?.name ?? '');
	let enabled = $state(initial?.enabled ?? true);
	let keep = $state(initial?.keep ?? 7);
	let includeKey = $state(initial?.include_key ?? false);

	// S3
	let endpoint = $state(saved('endpoint'));
	let region = $state(saved('region'));
	let bucket = $state(saved('bucket'));
	let prefix = $state(saved('prefix'));
	let accessKey = $state(saved('access_key_id'));
	let pathStyle = $state(initial?.settings?.path_style === true);
	// WebDAV
	let url = $state(saved('url'));
	// WebDAV and SFTP
	let username = $state(saved('username'));
	// SFTP
	let host = $state(saved('host'));
	let port = $state(typeof initial?.settings?.port === 'number' ? (initial.settings.port as number) : 22);
	let directory = $state(saved('directory'));
	let hostKey = $state(saved('host_key'));

	// Secrets: always typed, never shown.
	let secretKey = $state('');
	let password = $state('');
	let privateKey = $state('');
	let keyPassphrase = $state('');

	/** The saved secrets still apply only to the kind they were saved for. */
	const hasSaved = $derived(editing && initial?.has_secret === true && kind === initial?.kind);
	const savedHint = $derived(hasSaved ? m.w5backups_form_secret_saved() : undefined);

	let errors = $state<Record<string, string>>({});
	let apiError = $state<unknown>(null);
	let saving = $state(false);

	function typedSecrets(): Record<string, string> {
		const out: Record<string, string> = {};
		if (kind === 's3' && secretKey) out.secret_access_key = secretKey;
		if ((kind === 'webdav' || kind === 'sftp') && password) out.password = password;
		if (kind === 'sftp' && privateKey.trim()) out.private_key = privateKey.trim();
		if (kind === 'sftp' && keyPassphrase) out.key_passphrase = keyPassphrase;
		return out;
	}

	function validate(): boolean {
		const found: Record<string, string> = {};
		const required = m.w5backups_form_required();
		if (!name.trim()) found.name = m.w5backups_form_name_required();
		if (!Number.isInteger(keep) || keep < 1 || keep > 365) found.keep = m.w5backups_form_keep_invalid();
		const typed = typedSecrets();
		if (kind === 's3') {
			if (!bucket.trim()) found.bucket = required;
			if (!accessKey.trim()) found.accessKey = required;
			if (!hasSaved && !typed.secret_access_key) found.secretKey = required;
		} else if (kind === 'webdav') {
			if (!url.trim()) found.url = required;
		} else if (kind === 'sftp') {
			if (!host.trim()) found.host = required;
			if (!username.trim()) found.username = required;
			if (!Number.isInteger(port) || port < 1 || port > 65535) found.port = m.w5backups_form_port_invalid();
			if (!hasSaved && !typed.password && !typed.private_key) found.password = m.w5backups_sftp_auth_help();
		}
		errors = found;
		return Object.keys(found).length === 0;
	}

	function settings(): BackupDestinationPayload['settings'] {
		if (kind === 's3') {
			return {
				endpoint: endpoint.trim(),
				region: region.trim(),
				bucket: bucket.trim(),
				prefix: prefix.trim(),
				access_key_id: accessKey.trim(),
				path_style: pathStyle
			};
		}
		if (kind === 'webdav') return { url: url.trim(), username: username.trim() };
		return {
			host: host.trim(),
			port,
			username: username.trim(),
			directory: directory.trim(),
			host_key: hostKey.trim() || null
		};
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		apiError = null;
		if (!validate()) return;
		saving = true;
		try {
			const typed = typedSecrets();
			const payload: BackupDestinationPayload = {
				name: name.trim(),
				kind: kind as BackupDestinationKind,
				enabled,
				keep,
				include_key: includeKey,
				settings: settings()
			};
			// Omitted: the server keeps what it has. Typed: they replace it.
			if (Object.keys(typed).length > 0 || !hasSaved) payload.secrets = typed;
			const result = initial
				? await updateBackupDestination(initial.id, payload)
				: await createBackupDestination(payload);
			onsaved(result);
		} catch (cause) {
			apiError = cause;
		} finally {
			saving = false;
		}
	}

	function clear(key: string) {
		if (errors[key]) errors = { ...errors, [key]: '' };
	}
</script>

<form
	class="rounded-[var(--radius-card)] border border-signal/40 bg-surface shadow-lift"
	aria-label={editing ? m.w5backups_form_edit_title({ name: initial?.name ?? '' }) : m.w5backups_form_new_title()}
	onsubmit={save}
	novalidate
>
	<div class="border-b border-line px-4 py-3 sm:px-5">
		<h4 class="text-base font-semibold tracking-tight text-ink">
			{editing ? m.w5backups_form_edit_title({ name: initial?.name ?? '' }) : m.w5backups_form_new_title()}
		</h4>
	</div>

	<div class="grid gap-5 px-4 py-4 sm:px-5">
		<div class="grid gap-4 sm:grid-cols-2">
			<Field label={m.w5backups_form_name()} for="offsite-name" required error={errors.name || null}>
				<input id="offsite-name" type="text" class="input" bind:value={name} autocomplete="off" disabled={saving} aria-invalid={errors.name ? 'true' : undefined} oninput={() => clear('name')} />
			</Field>
			<Segmented legend={m.w5backups_form_kind()} name="offsite-kind" options={kindOptions} bind:value={kind} disabled={saving} />
		</div>

		{#if kind === 's3'}
			<fieldset class="grid gap-4 border-t border-line pt-5 sm:grid-cols-2">
				<legend class="sr-only">{m.w5backups_kind_s3()}</legend>
				<Field label={m.w5backups_s3_endpoint()} for="offsite-endpoint" help={m.w5backups_s3_endpoint_help()} class="sm:col-span-2">
					<input id="offsite-endpoint" type="url" class="input" bind:value={endpoint} autocomplete="off" spellcheck="false" placeholder="https://s3.eu-central-003.backblazeb2.com" disabled={saving} />
				</Field>
				<Field label={m.w5backups_s3_bucket()} for="offsite-bucket" required error={errors.bucket || null}>
					<input id="offsite-bucket" type="text" class="input" bind:value={bucket} autocomplete="off" spellcheck="false" disabled={saving} oninput={() => clear('bucket')} />
				</Field>
				<Field label={m.w5backups_s3_region()} for="offsite-region" help={m.w5backups_s3_region_help()}>
					<input id="offsite-region" type="text" class="input" bind:value={region} autocomplete="off" spellcheck="false" placeholder="us-east-1" disabled={saving} />
				</Field>
				<Field label={m.w5backups_s3_prefix()} for="offsite-prefix" help={m.w5backups_s3_prefix_help()} class="sm:col-span-2">
					<input id="offsite-prefix" type="text" class="input" bind:value={prefix} autocomplete="off" spellcheck="false" placeholder="dumbmonit/home" disabled={saving} />
				</Field>
				<Field label={m.w5backups_s3_access_key()} for="offsite-access-key" required error={errors.accessKey || null}>
					<input id="offsite-access-key" type="text" class="input font-mono" bind:value={accessKey} autocomplete="off" spellcheck="false" disabled={saving} oninput={() => clear('accessKey')} />
				</Field>
				<Field label={m.w5backups_s3_secret_key()} for="offsite-secret-key" required={!hasSaved} help={savedHint} error={errors.secretKey || null}>
					<PasswordInput id="offsite-secret-key" bind:value={secretKey} autocomplete="off" placeholder={savedHint} disabled={saving} invalid={!!errors.secretKey} oninput={() => clear('secretKey')} />
				</Field>
				<Field label={m.w5backups_s3_path_style()} for="offsite-path-style" inline help={m.w5backups_s3_path_style_help()} class="sm:col-span-2">
					<Toggle id="offsite-path-style" bind:checked={pathStyle} disabled={saving} label={m.w5backups_s3_path_style()} />
				</Field>
			</fieldset>
		{:else if kind === 'webdav'}
			<fieldset class="grid gap-4 border-t border-line pt-5 sm:grid-cols-2">
				<legend class="sr-only">{m.w5backups_kind_webdav()}</legend>
				<Field label={m.w5backups_webdav_url()} for="offsite-url" required help={m.w5backups_webdav_url_help()} error={errors.url || null} class="sm:col-span-2">
					<input id="offsite-url" type="url" class="input" bind:value={url} autocomplete="off" spellcheck="false" placeholder="https://cloud.example.com/remote.php/dav/files/me/Backups" disabled={saving} oninput={() => clear('url')} />
				</Field>
				<Field label={m.w5backups_username()} for="offsite-username">
					<input id="offsite-username" type="text" class="input" bind:value={username} autocomplete="off" spellcheck="false" disabled={saving} />
				</Field>
				<Field label={m.w5backups_password()} for="offsite-password" help={savedHint ?? m.w5backups_webdav_password_help()}>
					<PasswordInput id="offsite-password" bind:value={password} autocomplete="off" placeholder={savedHint} disabled={saving} />
				</Field>
			</fieldset>
		{:else}
			<fieldset class="grid gap-4 border-t border-line pt-5 sm:grid-cols-[1fr_8rem]">
				<legend class="sr-only">{m.w5backups_kind_sftp()}</legend>
				<Field label={m.w5backups_sftp_host()} for="offsite-host" required error={errors.host || null}>
					<input id="offsite-host" type="text" class="input" bind:value={host} autocomplete="off" spellcheck="false" placeholder="nas.home.lan" disabled={saving} oninput={() => clear('host')} />
				</Field>
				<Field label={m.w5backups_sftp_port()} for="offsite-port" error={errors.port || null}>
					<input id="offsite-port" type="number" min="1" max="65535" step="1" class="input tnum" bind:value={port} disabled={saving} oninput={() => clear('port')} />
				</Field>
				<Field label={m.w5backups_username()} for="offsite-sftp-user" required error={errors.username || null} class="sm:col-span-2">
					<input id="offsite-sftp-user" type="text" class="input" bind:value={username} autocomplete="off" spellcheck="false" disabled={saving} oninput={() => clear('username')} />
				</Field>
				<Field label={m.w5backups_sftp_directory()} for="offsite-directory" help={m.w5backups_sftp_directory_help()} class="sm:col-span-2">
					<input id="offsite-directory" type="text" class="input font-mono" bind:value={directory} autocomplete="off" spellcheck="false" placeholder="/volume1/backups/dumbmonit" disabled={saving} />
				</Field>
				<Field label={m.w5backups_sftp_host_key()} for="offsite-host-key" help={m.w5backups_sftp_host_key_help()} class="sm:col-span-2">
					<input id="offsite-host-key" type="text" class="input font-mono" bind:value={hostKey} autocomplete="off" spellcheck="false" placeholder="SHA256:…" disabled={saving} />
				</Field>
				<p class="text-[0.8125rem] text-ink-2 sm:col-span-2">{hasSaved ? m.w5backups_form_secrets_replace() : m.w5backups_sftp_auth_help()}</p>
				<Field label={m.w5backups_password()} for="offsite-sftp-password" error={errors.password || null} class="sm:col-span-2">
					<PasswordInput id="offsite-sftp-password" bind:value={password} autocomplete="off" placeholder={savedHint} disabled={saving} invalid={!!errors.password} oninput={() => clear('password')} />
				</Field>
				<Field label={m.w5backups_sftp_private_key()} for="offsite-private-key" help={m.w5backups_sftp_private_key_help()} class="sm:col-span-2">
					<textarea id="offsite-private-key" class="input min-h-24 font-mono text-[0.75rem]" rows="4" bind:value={privateKey} autocomplete="off" spellcheck="false" placeholder={savedHint ?? '-----BEGIN OPENSSH PRIVATE KEY-----'} disabled={saving} oninput={() => clear('password')}></textarea>
				</Field>
				<Field label={m.w5backups_sftp_key_passphrase()} for="offsite-key-passphrase" class="sm:col-span-2">
					<PasswordInput id="offsite-key-passphrase" bind:value={keyPassphrase} autocomplete="off" disabled={saving} />
				</Field>
			</fieldset>
		{/if}

		<div class="grid gap-4 border-t border-line pt-5 sm:grid-cols-2">
			<Field label={m.w5backups_form_keep()} for="offsite-keep" help={m.w5backups_form_keep_help()} error={errors.keep || null}>
				<input id="offsite-keep" type="number" min="1" max="365" step="1" class="input tnum max-w-[8rem]" bind:value={keep} disabled={saving} oninput={() => clear('keep')} />
			</Field>
			<Field label={m.w5backups_form_enabled()} for="offsite-enabled" inline>
				<Toggle id="offsite-enabled" bind:checked={enabled} disabled={saving} label={m.w5backups_form_enabled()} />
			</Field>
			<div class="grid gap-2 sm:col-span-2">
				<Field label={m.w5backups_form_include_key()} for="offsite-include-key" inline>
					<Toggle id="offsite-include-key" bind:checked={includeKey} disabled={saving} label={m.w5backups_form_include_key()} />
				</Field>
				<p class={`text-[0.8125rem] ${includeKey ? 'text-warning' : 'text-ink-2'}`}>{m.w5backups_form_include_key_warning()}</p>
			</div>
		</div>

		{#if apiError}
			<ErrorNotice error={apiError} title={m.w5backups_form_save_error()} />
		{/if}
	</div>

	<div class="flex flex-wrap items-center justify-end gap-2 border-t border-line px-4 py-3 sm:px-5">
		<Button variant="ghost" onclick={oncancel} disabled={saving}>{m.w5backups_form_cancel()}</Button>
		<Button type="submit" variant="primary" loading={saving}>{m.w5backups_form_save()}</Button>
	</div>
</form>
