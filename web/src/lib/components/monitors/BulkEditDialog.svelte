<script lang="ts">
	/**
	 * "Edit" of the monitors list's selection: the check interval, the
	 * notification channels (replace, add, remove), free tags (add, remove) and
	 * the folder, sent as one request (`PATCH /api/targets/bulk`). Only what is
	 * changed here is applied; the rest of each monitor stays as it is. The
	 * server answers one verdict per monitor: the list reports those it left
	 * as they were, and keeps them selected for a retry.
	 */
	import { tick } from 'svelte';
	import { X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { listChannels, type Channel, type Target } from '#lib/api/index.js';
	import { bulkUpdateTargets, type TargetBulkReport } from '#lib/api/targetsBulk.js';
	import { Button, Chip, ErrorNotice, Field, Segmented } from '#lib/ui/index.js';
	import { INTERVALS } from '#lib/components/device-form/kinds.js';
	import { KEEP, NEW_FOLDER, NO_FOLDER, buildBulkPatch, parseTag, type ChannelMode } from './bulk-edit.js';

	interface Props {
		/** The selection, as on screen. */
		targets: Target[];
		folderChoices: string[];
		/** Free tag keys the selection carries, offered for removal. */
		tagKeys: string[];
		ondone: (report: TargetBulkReport) => void;
	}

	let { targets, folderChoices, tagKeys, ondone }: Props = $props();

	let open = $state(false);
	let dialog = $state<HTMLDivElement | null>(null);
	let returnFocus: HTMLElement | null = null;

	let interval = $state(KEEP);
	/** A `ChannelMode`; a plain string for the segmented control's binding. */
	let channelMode = $state<string>('keep');
	let channelPicks = $state<Set<number>>(new Set());
	let tagToAdd = $state('');
	let tagsToRemove = $state<Set<string>>(new Set());
	let folder = $state(KEEP);
	let newFolder = $state('');
	let busy = $state(false);
	let error = $state<unknown>(null);

	let channels = $state<Channel[]>([]);
	/** Set once the list came back (or could not), so "no channel yet" never flashes. */
	let channelsRead = $state(false);

	export function show() {
		returnFocus = (document.activeElement as HTMLElement | null) ?? null;
		interval = KEEP;
		channelMode = 'keep';
		channelPicks = new Set();
		tagToAdd = '';
		tagsToRemove = new Set();
		folder = KEEP;
		newFolder = '';
		error = null;
		open = true;
		channelsRead = false;
		listChannels()
			.then((list) => (channels = list))
			.catch(() => (channels = []))
			.finally(() => (channelsRead = true));
		void tick().then(() => dialog?.focus());
	}

	function close() {
		open = false;
		returnFocus?.focus();
	}

	const patch = $derived(
		buildBulkPatch({ interval, channelMode: channelMode as ChannelMode, channels: channelPicks, tagToAdd, tagsToRemove, folder, newFolder })
	);
	const tagProblem = $derived(tagToAdd.trim() !== '' && parseTag(tagToAdd) === null ? m.w5monitors_bulkedit_tag_key_needed() : null);

	const modes = $derived([
		{ value: 'keep', label: m.w5monitors_bulkedit_channels_keep() },
		{ value: 'set', label: m.w5monitors_bulkedit_channels_set() },
		{ value: 'add', label: m.w5monitors_bulkedit_channels_add() },
		{ value: 'remove', label: m.w5monitors_bulkedit_channels_remove() }
	]);
	const channelsHelp = $derived(
		channelMode === 'set'
			? m.w5monitors_bulkedit_channels_help_set()
			: channelMode === 'keep'
				? m.w5monitors_bulkedit_channels_help_keep()
				: undefined
	);

	function toggle<T>(set: Set<T>, value: T): Set<T> {
		const next = new Set(set);
		if (next.has(value)) next.delete(value);
		else next.add(value);
		return next;
	}

	async function apply(event: SubmitEvent) {
		event.preventDefault();
		if (!patch || busy || tagProblem || targets.length === 0) return;
		busy = true;
		error = null;
		try {
			const report = await bulkUpdateTargets(
				targets.map((t) => t.id),
				patch
			);
			open = false;
			ondone(report);
		} catch (cause) {
			error = cause;
		} finally {
			busy = false;
		}
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			close();
		} else if (event.key === 'Tab' && dialog) {
			const items = [...dialog.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select:not([disabled])')];
			if (items.length === 0) return;
			const first = items[0];
			const final = items[items.length - 1];
			const active = document.activeElement;
			if (event.shiftKey && (active === first || active === dialog)) {
				event.preventDefault();
				final.focus();
			} else if (!event.shiftKey && active === final) {
				event.preventDefault();
				first.focus();
			}
		}
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-[60] bg-black/50" aria-hidden="true" onclick={close}></div>
	<div
		bind:this={dialog}
		role="dialog"
		aria-modal="true"
		aria-labelledby="bulk-edit-title"
		tabindex="-1"
		{onkeydown}
		class="fixed inset-x-0 bottom-0 z-[62] max-h-[92dvh] overflow-y-auto rounded-t-[var(--radius-card)] border border-line bg-surface p-5 pb-[calc(1.25rem+var(--safe-bottom))] text-ink shadow-float outline-none sm:inset-x-auto sm:top-1/2 sm:bottom-auto sm:left-1/2 sm:w-[36rem] sm:max-w-[calc(100vw-2rem)] sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-[var(--radius-card)] sm:pb-5"
	>
		<div class="flex items-start justify-between gap-3">
			<div class="min-w-0">
				<h2 id="bulk-edit-title" class="text-lg font-semibold">{m.w5monitors_bulkedit_title()}</h2>
				<p class="mt-0.5 text-sm text-ink-2">{m.w5monitors_bulkedit_lead({ count: targets.length })}</p>
			</div>
			<button
				type="button"
				class="-mt-1.5 -mr-2 inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink"
				aria-label={m.w5monitors_bulkedit_close()}
				onclick={close}
			>
				<X class="size-4" aria-hidden="true" />
			</button>
		</div>

		{#if error}
			<ErrorNotice {error} title={m.w5monitors_bulkedit_error()} class="mt-4" />
		{/if}

		<form class="mt-4 grid gap-5" onsubmit={apply}>
			<Field label={m.w5monitors_bulkedit_interval_label()} for="bulk-interval">
				<select id="bulk-interval" class="input" bind:value={interval}>
					<option value={KEEP}>{m.w5monitors_bulkedit_keep()}</option>
					{#each INTERVALS as choice (choice.value)}
						<option value={String(choice.value)}>{choice.label}</option>
					{/each}
				</select>
			</Field>

			<div class="grid gap-2">
				<Segmented legend={m.w5monitors_bulkedit_channels_label()} options={modes} bind:value={channelMode} name="bulk-channel-mode" help={channelsHelp} />
				{#if channelMode !== 'keep'}
					{#if channelsRead && channels.length === 0}
						<p class="text-sm text-ink-2">{m.w5monitors_bulkedit_channels_none()}</p>
					{:else}
						<fieldset class="grid gap-0.5 rounded-[var(--radius-card)] border border-line p-2">
							<legend class="sr-only">{m.w5monitors_bulkedit_channels_label()}</legend>
							{#each channels as channel (channel.id)}
								<label class="flex min-h-10 items-center gap-3 rounded-md px-1 text-sm text-ink hover:bg-surface-2/60">
									<input
										type="checkbox"
										class="size-4 shrink-0 accent-[var(--color-signal)]"
										checked={channelPicks.has(channel.id)}
										onchange={() => (channelPicks = toggle(channelPicks, channel.id))}
									/>
									<span class="min-w-0 break-words">{channel.name}</span>
									{#if !channel.enabled}
										<span class="text-[0.8125rem] text-ink-3">{m.w5monitors_bulkedit_channel_off()}</span>
									{/if}
								</label>
							{/each}
						</fieldset>
					{/if}
				{/if}
			</div>

			<div class="grid gap-3">
				<Field label={m.w5monitors_bulkedit_tag_add_label()} for="bulk-tag-add" help={m.w5monitors_bulkedit_tag_add_help()} error={tagProblem}>
					<input id="bulk-tag-add" class="input font-mono text-[0.8125rem]" autocomplete="off" spellcheck="false" maxlength="265" bind:value={tagToAdd} placeholder="room=rack-1" />
				</Field>
				{#if tagKeys.length > 0}
					<div class="grid gap-1.5" role="group" aria-labelledby="bulk-tag-remove-label">
						<p id="bulk-tag-remove-label" class="text-sm font-semibold text-ink">{m.w5monitors_bulkedit_tag_remove_label()}</p>
						<div class="flex flex-wrap gap-1.5">
							{#each tagKeys as key (key)}
								<Chip pressed={tagsToRemove.has(key)} onclick={() => (tagsToRemove = toggle(tagsToRemove, key))}>
									<span class="max-w-[12rem] truncate">{tagsToRemove.has(key) ? m.w5monitors_bulkedit_tag_removing({ key }) : key}</span>
								</Chip>
							{/each}
						</div>
					</div>
				{/if}
			</div>

			<div class="grid gap-2 sm:grid-cols-2">
				<Field label={m.w5monitors_bulkedit_folder_label()} for="bulk-folder">
					<select id="bulk-folder" class="input" bind:value={folder}>
						<option value={KEEP}>{m.w5monitors_bulkedit_keep()}</option>
						<option value={NO_FOLDER}>{m.devices_list_bulk_no_folder()}</option>
						{#each folderChoices as name (name)}
							<option value={name}>{name}</option>
						{/each}
						<option value={NEW_FOLDER}>{m.w5monitors_bulkedit_folder_new()}</option>
					</select>
				</Field>
				{#if folder === NEW_FOLDER}
					<Field label={m.w5monitors_bulkedit_folder_name()} for="bulk-folder-new">
						<input id="bulk-folder-new" class="input" autocomplete="off" maxlength="60" bind:value={newFolder} placeholder={m.devices_list_bulk_folder_placeholder()} />
					</Field>
				{/if}
			</div>

			<div class="flex flex-wrap items-center justify-end gap-2 border-t border-line pt-4">
				{#if !patch}
					<p class="mr-auto text-[0.8125rem] text-ink-2">{m.w5monitors_bulkedit_nothing()}</p>
				{/if}
				<Button variant="ghost" onclick={close}>{m.w5monitors_bulkedit_cancel()}</Button>
				<Button type="submit" variant="primary" loading={busy} disabled={!patch || !!tagProblem || targets.length === 0}>
					{m.w5monitors_bulkedit_apply()}
				</Button>
			</div>
		</form>
	</div>
{/if}
