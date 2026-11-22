<script lang="ts">
	/**
	 * Settings → Account & security → Signed-in devices: the open sessions of
	 * the current account (browser, address, when it signed in, last activity),
	 * with a way to sign out one of them, or all the others at once. The
	 * session in use is marked and cannot be closed here: "Sign out" below
	 * does that. Mounted inside `SecuritySection` with one line.
	 */
	import { MonitorSmartphone } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { ApiError, type Session, type SessionVia } from '#lib/api/index.js';
	import { listSessions, revokeOtherSessions, revokeSession } from '#lib/api/sessions.js';
	import { formatDateTime, formatRelative } from '#lib/format.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Confirm, ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';

	let sessions = $state<Session[] | null>(null);
	let loadError = $state<unknown>(null);
	let busyId = $state<string | null>(null);
	let rowError = $state<{ id: string; cause: unknown } | null>(null);
	let revokingOthers = $state(false);
	let othersError = $state<unknown>(null);
	/** How many sessions the last "sign out the others" closed, to confirm it. */
	let othersClosed = $state<number | null>(null);

	async function load(signal?: AbortSignal) {
		loadError = null;
		try {
			sessions = await listSessions(signal);
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			loadError = cause;
		}
	}

	$effect(() => {
		if (!auth.available || !auth.authenticated) return;
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	const others = $derived((sessions ?? []).filter((session) => !session.current));

	const VIA_LABEL: Record<SessionVia, () => string> = {
		password: () => m.settings_sessions_via_password(),
		passkey: () => m.settings_sessions_via_passkey(),
		ldap: () => 'LDAP',
		oidc: () => auth.oidc.provider_name || 'SSO',
		saml: () => auth.saml.label || 'SAML'
	};

	/** "Firefox on Linux", or what is known of it. */
	function device(session: Session): string {
		if (session.browser && session.os) return m.settings_sessions_device({ browser: session.browser, os: session.os });
		if (session.browser) return session.browser;
		if (session.os) return session.os;
		return m.settings_sessions_device_unknown();
	}

	async function revoke(session: Session) {
		busyId = session.id;
		rowError = null;
		othersClosed = null;
		try {
			await revokeSession(session.id);
			sessions = (sessions ?? []).filter((s) => s.id !== session.id);
		} catch (cause) {
			// Already gone (expired, closed elsewhere): the list was just stale.
			if (cause instanceof ApiError && cause.status === 404) {
				sessions = (sessions ?? []).filter((s) => s.id !== session.id);
			} else {
				rowError = { id: session.id, cause };
			}
		} finally {
			busyId = null;
		}
	}

	async function revokeOthers() {
		revokingOthers = true;
		othersError = null;
		othersClosed = null;
		try {
			othersClosed = await revokeOtherSessions();
			await load();
		} catch (cause) {
			othersError = cause;
		} finally {
			revokingOthers = false;
		}
	}
</script>

{#if auth.available && auth.authenticated}
	<div class="mt-6 border-t border-line pt-5 max-sm:mt-4 max-sm:pt-4" data-testid="sessions">
		<div class="flex flex-wrap items-start justify-between gap-3">
			<div class="min-w-0">
				<p class="text-sm font-semibold text-ink">{m.settings_sessions_title()}</p>
				<p class="mt-0.5 text-[0.8125rem] text-ink-2 max-sm:hidden">{m.settings_sessions_hint()}</p>
			</div>
			{#if others.length > 0}
				<Confirm variant="secondary" confirmLabel={m.settings_sessions_revoke_others_confirm()} loading={revokingOthers} onconfirm={revokeOthers}>
					{m.settings_sessions_revoke_others()}
				</Confirm>
			{/if}
		</div>

		<div aria-live="polite">
			{#if othersClosed !== null}
				<Plate class="mt-3" tone="signal" draw label={m.settings_sessions_revoked_others({ count: othersClosed })} />
			{/if}
		</div>
		{#if othersError}
			<ErrorNotice error={othersError} title={m.settings_sessions_revoke_error()} class="mt-3" />
		{/if}

		{#if loadError}
			<ErrorNotice error={loadError} title={m.settings_sessions_load_error()} onretry={() => void load()} class="mt-3" />
		{:else if sessions === null}
			<Skeleton class="mt-3 h-14 w-full" rows={2} />
		{:else}
			<ul class="mt-3 divide-y divide-line rounded-[var(--radius-card)] border border-line" role="list">
				{#each sessions as session (session.id)}
					<li class="px-4 py-3">
						<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
							<MonitorSmartphone class="size-5 shrink-0 text-ink-3" aria-hidden="true" />
							<div class="min-w-0 flex-[1_1_14rem]">
								<div class="flex flex-wrap items-center gap-2">
									<span class="font-semibold text-ink" title={session.user_agent ?? undefined}>{device(session)}</span>
									{#if session.current}
										<Plate tone="signal" bare label={m.settings_sessions_current()} />
									{/if}
									{#if session.via}
										<Plate tone="ghost" bare label={VIA_LABEL[session.via]?.() ?? session.via} />
									{/if}
								</div>
								<p class="mt-1 text-sm text-ink-2">
									<time class="tnum" title={formatDateTime(session.created_at)}>{m.settings_sessions_signed_in({ when: formatRelative(session.created_at) })}</time>
									{#if session.last_seen_at}
										· <time class="tnum" title={formatDateTime(session.last_seen_at)}>{m.settings_sessions_last_active({ when: formatRelative(session.last_seen_at) })}</time>
									{/if}
									{#if session.ip}
										· <span class="tnum font-mono break-all">{session.ip}</span>
									{/if}
								</p>
							</div>
							{#if !session.current}
								<Confirm confirmLabel={m.settings_sessions_revoke_confirm()} loading={busyId === session.id} onconfirm={() => revoke(session)}>
									{m.settings_sessions_revoke()}
								</Confirm>
							{/if}
						</div>
						{#if rowError?.id === session.id}
							<ErrorNotice error={rowError.cause} title={m.settings_sessions_revoke_error()} class="mt-3" />
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/if}
