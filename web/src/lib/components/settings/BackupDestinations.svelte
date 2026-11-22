<script lang="ts">
	/**
	 * Settings → Backup → Off-site copies: where each local backup is uploaded
	 * afterwards (S3-compatible, WebDAV, SFTP), the last upload of each, and
	 * the test that writes, lists and deletes a small file.
	 *
	 * An SFTP server whose host key is not confirmed is never authenticated
	 * to: the test shows the fingerprint it presented, and the administrator
	 * confirms it here after comparing it with the server's own.
	 */
	import { KeyRound, Pencil, Plus, ShieldAlert, Trash2, Upload } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import {
		deleteBackupDestination,
		destinationWhere,
		listBackupDestinations,
		testBackupDestination,
		updateBackupDestination,
		type BackupDestination,
		type BackupDestinationKind,
		type BackupDestinationTest,
		type SftpDestinationSettings
	} from '#lib/api/backupDestinations.js';
	import { formatBinaryBytes, formatDateTime, formatRelative } from '#lib/format.js';
	import { startPolling } from '#lib/poll.js';
	import { Button, Confirm, Disclosure, ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';
	import BackupDestinationForm from './BackupDestinationForm.svelte';

	interface Props {
		/** Bumped by the parent after "Back up now": reload and follow the upload. */
		refresh?: number;
	}

	let { refresh = 0 }: Props = $props();

	let destinations = $state<BackupDestination[]>([]);
	let uploading = $state(false);
	let loaded = $state(false);
	let error = $state<unknown>(null);
	let actionError = $state<unknown>(null);
	/** `null`: no form; `'new'`: adding; a number: editing that destination. */
	let editing = $state<'new' | number | null>(null);
	let tests = $state<Record<number, BackupDestinationTest>>({});
	let testing = $state<number | null>(null);
	let trusting = $state<number | null>(null);
	let deleting = $state<number | null>(null);

	async function load(signal?: AbortSignal): Promise<boolean | void> {
		try {
			const list = await listBackupDestinations(signal);
			destinations = list.destinations;
			uploading = list.uploading;
			error = null;
		} catch (cause) {
			if (signal?.aborted) return;
			error = cause;
			return false;
		} finally {
			loaded = true;
		}
	}

	// First read, and again whenever the parent ran a backup.
	$effect(() => {
		void refresh;
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	// While an upload runs, follow it until it is done.
	$effect(() => {
		if (!uploading) return;
		return startPolling(load, 3000, { immediate: false });
	});

	const KIND_LABEL: Record<string, () => string> = {
		s3: m.w5backups_kind_s3,
		webdav: m.w5backups_kind_webdav,
		sftp: m.w5backups_kind_sftp
	};
	const kindLabel = (kind: string) => KIND_LABEL[kind]?.() ?? kind;

	const summary = $derived.by(() => {
		if (!loaded || error) return undefined;
		if (destinations.length === 0) return m.w5backups_summary_none();
		if (uploading) return m.w5backups_uploading();
		if (destinations.some((d) => d.enabled && d.last && !d.last.ok)) return m.w5backups_summary_failed();
		return undefined;
	});

	function saved(destination: BackupDestination) {
		const index = destinations.findIndex((d) => d.id === destination.id);
		destinations =
			index >= 0 ? destinations.map((d) => (d.id === destination.id ? destination : d)) : [...destinations, destination];
		const next = { ...tests };
		delete next[destination.id];
		tests = next;
		editing = null;
	}

	async function test(destination: BackupDestination): Promise<boolean> {
		testing = destination.id;
		actionError = null;
		try {
			tests = { ...tests, [destination.id]: await testBackupDestination(destination.id) };
			return true;
		} catch (cause) {
			actionError = cause;
			return false;
		} finally {
			testing = null;
		}
	}

	/** Pins the fingerprint the server presented, then tests again with it. */
	async function trust(destination: BackupDestination, fingerprint: string): Promise<boolean> {
		trusting = destination.id;
		actionError = null;
		try {
			const updated = await updateBackupDestination(destination.id, {
				name: destination.name,
				kind: destination.kind as BackupDestinationKind,
				enabled: destination.enabled,
				keep: destination.keep,
				include_key: destination.include_key,
				settings: { ...destination.settings, host_key: fingerprint } as unknown as SftpDestinationSettings
			});
			saved(updated);
		} catch (cause) {
			actionError = cause;
			return false;
		} finally {
			trusting = null;
		}
		return test(destinations.find((d) => d.id === destination.id) ?? destination);
	}

	async function remove(destination: BackupDestination): Promise<boolean> {
		deleting = destination.id;
		actionError = null;
		try {
			await deleteBackupDestination(destination.id);
			destinations = destinations.filter((d) => d.id !== destination.id);
			if (editing === destination.id) editing = null;
			return true;
		} catch (cause) {
			actionError = cause;
			return false;
		} finally {
			deleting = null;
		}
	}
</script>

<Disclosure title={m.w5backups_title()} {summary}>
	<p class="mt-3 max-w-prose text-sm text-ink-2">{m.w5backups_intro()}</p>

	{#if error}
		<ErrorNotice {error} title={m.w5backups_read_error()} onretry={() => void load()} class="mt-3" />
	{:else if !loaded}
		<Skeleton class="mt-3 h-16 w-full" />
	{:else}
		{#if destinations.length === 0 && editing === null}
			<p class="mt-3 text-sm text-ink-2">{m.w5backups_empty()}</p>
		{/if}

		{#if destinations.length > 0}
			<ul class="mt-3 divide-y divide-line rounded-[var(--radius-card)] border border-line" role="list" aria-live="polite">
				{#each destinations as destination (destination.id)}
					{@const result = tests[destination.id]}
					<li class="grid gap-2 px-4 py-3 text-sm">
						{#if editing === destination.id}
							<BackupDestinationForm {destination} onsaved={saved} oncancel={() => (editing = null)} />
						{:else}
							<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
								<span class="font-semibold text-ink">{destination.name}</span>
								<span class="text-ink-3">{kindLabel(destination.kind)}</span>
								{#if !destination.enabled}
									<Plate tone="ghost" label={m.w5backups_paused()} />
								{/if}
								{#if destination.include_key}
									<Plate tone="advisory">
										<KeyRound class="size-3.5" aria-hidden="true" />
										{m.w5backups_with_key()}
									</Plate>
								{/if}
							</div>
							<p class="min-w-0 font-mono text-[0.75rem] break-all text-ink-2">{destinationWhere(destination)}</p>

							{#if destination.last}
								{@const last = destination.last}
								<div class="flex flex-wrap items-center gap-2">
									<Plate tone={last.ok ? 'signal' : 'warning'} label={last.ok ? m.w5backups_last_ok() : m.w5backups_last_failed()} />
									<span class="text-ink-2">
										<time class="tnum" title={formatDateTime(last.at)}>{formatRelative(last.at)}</time>
										{#if last.ok}· {formatBinaryBytes(last.bytes)}{/if}
									</span>
								</div>
								{#if last.error}
									<p class="text-warning break-words">{last.error}</p>
								{/if}
							{:else if uploading && destination.enabled}
								<Plate tone="info" label={m.w5backups_uploading()} pulse />
							{:else}
								<p class="text-ink-3">{m.w5backups_never()}</p>
							{/if}

							{#if result}
								<div class="grid gap-2 rounded-[var(--radius-card)] border border-line bg-surface-2 p-3">
									<div class="flex flex-wrap items-center gap-2">
										<Plate tone={result.ok ? 'signal' : 'warning'} label={result.ok ? m.w5backups_test_ok() : m.w5backups_test_failed()} />
										{#if !result.ok && !result.host_key}<span class="text-ink-2 break-words">{result.message}</span>{/if}
									</div>
									{#if result.host_key}
										<div class="flex items-start gap-2">
											<ShieldAlert class={`mt-0.5 size-4 shrink-0 ${result.host_key_changed ? 'text-warning' : 'text-ink-2'}`} aria-hidden="true" />
											<p class={result.host_key_changed ? 'text-warning' : 'text-ink-2'}>
												{result.host_key_changed ? m.w5backups_hostkey_changed() : m.w5backups_hostkey_unknown()}
											</p>
										</div>
										<code class="font-mono text-[0.8125rem] break-all text-ink">{result.host_key}</code>
										<div>
											{#if result.host_key_changed}
												<Confirm variant="secondary" size="md" confirmLabel={m.w5backups_hostkey_trust_confirm()} loading={trusting === destination.id} onconfirm={() => trust(destination, result.host_key ?? '')}>
													{m.w5backups_hostkey_trust()}
												</Confirm>
											{:else}
												<Button variant="secondary" loading={trusting === destination.id} onclick={() => trust(destination, result.host_key ?? '')}>
													{m.w5backups_hostkey_trust()}
												</Button>
											{/if}
										</div>
									{/if}
								</div>
							{/if}

							<div class="flex flex-wrap items-center gap-2">
								<Button variant="secondary" size="sm" loading={testing === destination.id} onclick={() => test(destination)} aria-label={m.w5backups_test_label({ name: destination.name })}>
									<Upload class="size-4" aria-hidden="true" />
									{m.w5backups_test()}
								</Button>
								<Button variant="ghost" size="sm" onclick={() => (editing = destination.id)} aria-label={m.w5backups_edit_label({ name: destination.name })}>
									<Pencil class="size-4" aria-hidden="true" />
									{m.w5backups_edit()}
								</Button>
								<Confirm confirmLabel={m.w5backups_delete_confirm()} loading={deleting === destination.id} onconfirm={() => remove(destination)}>
									<Trash2 class="size-4" aria-hidden="true" />
									{m.w5backups_delete()}
								</Confirm>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		{#if actionError}
			<ErrorNotice error={actionError} title={m.w5backups_action_error()} class="mt-3" />
		{/if}

		<div class="mt-3">
			{#if editing === 'new'}
				<BackupDestinationForm onsaved={saved} oncancel={() => (editing = null)} />
			{:else}
				<Button variant="secondary" onclick={() => (editing = 'new')}>
					<Plus class="size-4" aria-hidden="true" />
					{m.w5backups_add()}
				</Button>
			{/if}
		</div>
	{/if}
</Disclosure>
