<script lang="ts">
	/**
	 * Settings → Account & security → Two-factor authentication.
	 *
	 * Self-contained: loads its own status, walks through the enrolment
	 * (password → QR code → first code → recovery codes shown once) and offers
	 * the way out (password again). Recovery codes can be replaced on their
	 * own (a code from the app, or the password) and saved as a text file.
	 * The audit log has its own section (`AuditLogSection`). Mounted inside
	 * `SecuritySection` with one line, and by `TotpSetupGate` with `required`
	 * while the instance policy holds the session at enrolment.
	 */
	import qrcode from 'qrcode-generator';
	import { Download, KeyRound, ShieldCheck, ShieldOff } from 'lucide-svelte';
	import { ApiError, type TotpEnrolment, type TotpStatus } from '#lib/api/index.js';
	import { disableTotp, enrolTotp, getTotpStatus, regenerateRecoveryCodes, verifyTotp } from '#lib/api/totp.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, CopyBlock, DocLink, ErrorNotice, Field, Plate, Skeleton } from '#lib/ui/index.js';
	import PasswordInput from './PasswordInput.svelte';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		/**
		 * Mounted by the two-factor gate: the session can do nothing else until
		 * the second factor is on, so the app only comes back once the recovery
		 * codes are saved, and the admin-only security log is left out.
		 */
		required?: boolean;
	}
	let { required = false }: Props = $props();

	let status = $state<TotpStatus | null>(null);
	let loadError = $state<unknown>(null);

	// Enrolment walk-through.
	let step = $state<'idle' | 'password' | 'scan' | 'codes' | 'disable' | 'regenerate'>('idle');
	/** Replacing the recovery codes: confirmed by a code from the app, or by the password. */
	let confirmWith = $state<'code' | 'password'>('code');
	let password = $state('');
	let passwordError = $state<string | null>(null);
	let enrolment = $state<TotpEnrolment | null>(null);
	let code = $state('');
	let codeError = $state<string | null>(null);
	let recoveryCodes = $state<string[]>([]);
	let busy = $state(false);
	let apiError = $state<unknown>(null);

	const canEnrol = $derived(auth.available && auth.user?.auth === 'password');

	async function load() {
		loadError = null;
		try {
			status = await getTotpStatus();
		} catch (cause) {
			loadError = cause;
		}
	}

	$effect(() => {
		if (canEnrol) void load();
	});

	const qrSvg = $derived.by(() => {
		if (!enrolment) return '';
		const qr = qrcode(0, 'M');
		qr.addData(enrolment.otpauth_uri);
		qr.make();
		return qr.createSvgTag({ cellSize: 4, margin: 2, scalable: true });
	});

	function start(next: 'password' | 'disable' | 'regenerate') {
		step = next;
		confirmWith = 'code';
		password = '';
		passwordError = null;
		apiError = null;
		code = '';
		codeError = null;
	}

	function cancel() {
		const leavingCodes = step === 'codes';
		step = 'idle';
		enrolment = null;
		password = '';
		code = '';
		recoveryCodes = [];
		apiError = null;
		// Held at enrolment, the app comes back only once the codes are saved.
		if (required && leavingCodes) void auth.refresh();
	}

	async function submitRegenerate(event: SubmitEvent) {
		event.preventDefault();
		codeError = null;
		passwordError = null;
		apiError = null;
		if (confirmWith === 'code' && !code.trim()) {
			codeError = m.settings_2fa_err_code();
			return;
		}
		if (confirmWith === 'password' && !password) {
			passwordError = m.settings_2fa_err_password();
			return;
		}
		busy = true;
		try {
			recoveryCodes = await regenerateRecoveryCodes(confirmWith === 'code' ? { code: code.trim() } : { password });
			code = '';
			password = '';
			step = 'codes';
			await load();
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 401) {
				if (confirmWith === 'code') codeError = m.settings_2fa_err_wrong_code();
				else passwordError = m.settings_2fa_err_wrong_password();
			} else apiError = cause;
		} finally {
			busy = false;
		}
	}

	/** Saves the codes on screen as a text file, built in the browser: nothing leaves the page. */
	function downloadCodes() {
		const account = auth.user?.username ?? '';
		const header = m.settings_2fa_codes_file_header({ account });
		const text = `${header}\n\n${recoveryCodes.join('\n')}\n`;
		const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
		const link = document.createElement('a');
		link.href = url;
		link.download = account ? `dumbmonit-recovery-codes-${account}.txt` : 'dumbmonit-recovery-codes.txt';
		document.body.append(link);
		link.click();
		link.remove();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}

	async function submitPassword(event: SubmitEvent) {
		event.preventDefault();
		passwordError = null;
		apiError = null;
		if (!password) {
			passwordError = m.settings_2fa_err_password();
			return;
		}
		busy = true;
		try {
			if (step === 'disable') {
				await disableTotp(password);
				password = '';
				step = 'idle';
				await load();
				await auth.refresh();
			} else {
				enrolment = await enrolTotp(password);
				password = '';
				step = 'scan';
			}
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 401) passwordError = m.settings_2fa_err_wrong_password();
			else apiError = cause;
		} finally {
			busy = false;
		}
	}

	async function submitCode(event: SubmitEvent) {
		event.preventDefault();
		codeError = null;
		apiError = null;
		if (!code.trim()) {
			codeError = m.settings_2fa_err_code();
			return;
		}
		busy = true;
		try {
			recoveryCodes = await verifyTotp(code.trim());
			code = '';
			step = 'codes';
			await load();
			// Held at enrolment, refreshing now would swap this screen for the
			// app before the codes are saved: that waits for "I saved them".
			if (!required) await auth.refresh();
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 401) codeError = cause.message;
			else apiError = cause;
		} finally {
			busy = false;
		}
	}
</script>

{#if canEnrol}
	<div class="mt-6 border-t border-line pt-5 max-sm:mt-4 max-sm:pt-4" data-testid="two-factor">
		<div class="flex flex-wrap items-start justify-between gap-3">
			<div>
				<p class="text-sm font-semibold text-ink">{m.settings_2fa_title()}</p>
				<DocLink page="using/settings/" />
			</div>
			{#if status === null && !loadError}
				<Skeleton class="h-7 w-24" />
			{:else if status?.enabled}
				<div class="flex flex-wrap items-center gap-2">
					<Plate tone="signal" label={m.settings_2fa_enabled()} />
					<Button size="sm" variant="secondary" disabled={step !== 'idle'} onclick={() => start('disable')}>
						<ShieldOff class="size-4" aria-hidden="true" />
						{m.settings_2fa_disable()}
					</Button>
				</div>
			{:else}
				<div class="flex flex-wrap items-center gap-2">
					<Plate tone="ghost" label={m.settings_2fa_off()} />
					<Button size="sm" variant="secondary" disabled={step !== 'idle'} onclick={() => start('password')}>
						<ShieldCheck class="size-4" aria-hidden="true" />
						{m.settings_2fa_setup()}
					</Button>
				</div>
			{/if}
		</div>

		{#if loadError}
			<div class="mt-3"><ErrorNotice error={loadError} title={m.settings_2fa_load_error()} /></div>
		{/if}

		{#if status?.enabled && step === 'idle'}
			<div class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-2">
				<p class="text-[0.8125rem] text-ink-2">
					{m.settings_2fa_recovery_left({ count: status.recovery_codes_left })}
					{#if status.recovery_codes_left === 0}
						{m.settings_2fa_recovery_none_replace()}
					{/if}
				</p>
				<Button size="sm" variant="ghost" onclick={() => start('regenerate')}>
					<KeyRound class="size-4" aria-hidden="true" />
					{m.settings_2fa_regenerate()}
				</Button>
			</div>
		{/if}

		{#if step === 'password' || step === 'disable'}
			<form class="mt-4 grid max-w-md gap-4" onsubmit={submitPassword} novalidate>
				<Field label={step === 'disable' ? m.settings_2fa_pw_disable() : m.settings_2fa_pw_start()} for="totp-password" error={passwordError}>
					<PasswordInput id="totp-password" bind:value={password} autocomplete="current-password" disabled={busy} invalid={!!passwordError} oninput={() => (passwordError = null)} />
				</Field>
				{#if apiError}
					<ErrorNotice error={apiError} title={m.settings_2fa_continue_error()} />
				{/if}
				<div class="flex flex-wrap items-center gap-2">
					<Button type="submit" variant={step === 'disable' ? 'danger' : 'primary'} loading={busy}>
						{step === 'disable' ? m.settings_2fa_disable_button() : m.settings_2fa_continue()}
					</Button>
					<Button type="button" variant="ghost" onclick={cancel}>{m.settings_2fa_cancel()}</Button>
				</div>
			</form>
		{:else if step === 'scan' && enrolment}
			<div class="mt-4 grid gap-5 md:grid-cols-[auto_1fr]">
				<div class="qr w-fit rounded-lg border border-line bg-white p-2" aria-label={m.settings_2fa_qr_label()}>
					<!-- eslint-disable-next-line svelte/no-at-html-tags -- SVG built locally from the enrolment URI, never from user content. -->
					{@html qrSvg}
				</div>
				<form class="grid max-w-md gap-4" onsubmit={submitCode} novalidate>
					<div class="text-[0.8125rem] text-ink-2">
						<p>{m.settings_2fa_step1()}</p>
						<p class="mt-1">{m.settings_2fa_step2()}</p>
						<details class="mt-2">
							<summary class="cursor-pointer text-ink">{m.settings_2fa_cant_scan()}</summary>
							<div class="mt-2"><CopyBlock value={enrolment.secret} label={m.settings_2fa_copy_key()} /></div>
							<p class="mt-1">{m.settings_2fa_key_details({ account: enrolment.account, issuer: enrolment.issuer })}</p>
						</details>
					</div>
					<Field label={m.settings_2fa_code_label()} for="totp-first-code" error={codeError}>
						<input id="totp-first-code" type="text" class="input tnum max-w-[12rem] tracking-[0.2em]" bind:value={code} autocomplete="one-time-code" inputmode="numeric" disabled={busy} aria-invalid={codeError ? 'true' : undefined} oninput={() => (codeError = null)} />
					</Field>
					{#if apiError}
						<ErrorNotice error={apiError} title={m.settings_2fa_confirm_error()} />
					{/if}
					<div class="flex flex-wrap items-center gap-2">
						<Button type="submit" variant="primary" loading={busy}>{m.settings_2fa_confirm_enable()}</Button>
						<Button type="button" variant="ghost" onclick={cancel}>{m.settings_2fa_cancel()}</Button>
					</div>
				</form>
			</div>
		{:else if step === 'regenerate'}
			<form class="mt-4 grid max-w-md gap-4" onsubmit={submitRegenerate} novalidate>
				<p class="text-[0.8125rem] text-ink-2">{m.settings_2fa_regenerate_intro()}</p>
				{#if confirmWith === 'code'}
					<Field label={m.settings_2fa_regenerate_code_label()} for="totp-regenerate-code" error={codeError}>
						<input id="totp-regenerate-code" type="text" class="input tnum max-w-[12rem] tracking-[0.2em]" bind:value={code} autocomplete="one-time-code" inputmode="numeric" disabled={busy} aria-invalid={codeError ? 'true' : undefined} oninput={() => (codeError = null)} />
					</Field>
				{:else}
					<Field label={m.settings_2fa_regenerate_password_label()} for="totp-regenerate-password" error={passwordError}>
						<PasswordInput id="totp-regenerate-password" bind:value={password} autocomplete="current-password" disabled={busy} invalid={!!passwordError} oninput={() => (passwordError = null)} />
					</Field>
				{/if}
				<div>
					<Button
						size="sm"
						variant="ghost"
						disabled={busy}
						onclick={() => {
							confirmWith = confirmWith === 'code' ? 'password' : 'code';
							codeError = null;
							passwordError = null;
						}}
					>
						{confirmWith === 'code' ? m.settings_2fa_regenerate_use_password() : m.settings_2fa_regenerate_use_code()}
					</Button>
				</div>
				{#if apiError}
					<ErrorNotice error={apiError} title={m.settings_2fa_regenerate_error()} />
				{/if}
				<div class="flex flex-wrap items-center gap-2">
					<Button type="submit" variant="primary" loading={busy}>{m.settings_2fa_regenerate_confirm()}</Button>
					<Button type="button" variant="ghost" onclick={cancel}>{m.settings_2fa_cancel()}</Button>
				</div>
			</form>
		{:else if step === 'codes'}
			<div class="mt-4 max-w-md">
				<Plate tone="signal" size="md" label={m.settings_2fa_is_on()} />
				<p class="mt-2 text-[0.8125rem] text-ink-2">{m.settings_2fa_codes_note()}</p>
				<div class="mt-3"><CopyBlock value={recoveryCodes.join('\n')} label={m.settings_2fa_copy_codes()} /></div>
				<div class="mt-3 flex flex-wrap items-center gap-2">
					<Button variant="secondary" onclick={downloadCodes}>
						<Download class="size-4" aria-hidden="true" />
						{m.settings_2fa_download_codes()}
					</Button>
					<Button variant="primary" onclick={cancel}>{m.settings_2fa_saved()}</Button>
				</div>
			</div>
		{/if}

	</div>
{/if}

<style>
	.qr :global(svg) {
		display: block;
		width: 10rem;
		height: 10rem;
	}
</style>
