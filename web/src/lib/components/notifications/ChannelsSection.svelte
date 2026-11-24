<script lang="ts">
	/**
	 * Alerts → Notifications: the channel list, and the add/edit form that
	 * opens above it. The kind catalogue and the list load together: without
	 * the catalogue we could neither name a kind nor build the form.
	 */
	import ClientAssign from '#lib/components/ClientAssign.svelte';
	import { BellRing, EllipsisVertical, Plus, Search } from 'lucide-svelte';
	import { deleteChannel, listChannelKinds, listChannels, testChannel, toApiError, updateChannel, type Channel } from '#lib/api/index.js';
	import { formatDateTime, formatRelative, pad2 } from '#lib/format.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ClickSpark, Confirm, EmptyState, ErrorNotice, Menu, Panel, Plate, Skeleton, Toggle } from '#lib/ui/index.js';
	import { kindIcon, normalizeKind, type KindInfo } from './kinds';
	import { withoutOnCall } from '#lib/components/oncall/gate.js';
	import { instanceStore } from '#lib/stores/instance.svelte.js';
	import ChannelForm from './ChannelForm.svelte';
	import ChannelErrorDetail from './ChannelErrorDetail.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import FileManagedBadge from '#lib/components/settings/FileManagedBadge.svelte';

	let kinds = $state<KindInfo[]>([]);
	let channels = $state<Channel[]>([]);
	// The on-call channel kind (and the channels made with it) only shows while on-call schedules are on.
	const pickKinds = $derived(withoutOnCall(kinds, instanceStore.oncallEnabled));
	const allChannels = $derived(withoutOnCall(channels, instanceStore.oncallEnabled));
	let search = $state('');
	const shownChannels = $derived.by(() => {
		const term = search.trim().toLowerCase();
		if (!term) return allChannels;
		return allChannels.filter((c) => `${c.name} ${kindLabel(c.kind)} ${c.kind}`.toLowerCase().includes(term));
	});
	let loading = $state(true);
	let error = $state<unknown>(null);

	async function load(signal?: AbortSignal) {
		loading = true;
		error = null;
		try {
			const [catalogue, list] = await Promise.all([listChannelKinds(signal), listChannels(signal)]);
			kinds = (Array.isArray(catalogue) ? catalogue : [])
				.filter((k) => k && typeof k.kind === 'string')
				.map(normalizeKind);
			channels = Array.isArray(list) ? list : [];
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

	/** "Last sent <time>" cut around the time, which stays a <time> element. */
	const lastSentParts = $derived(m.notifications_channels_last_sent({ when: '\u0000' }).split('\u0000'));

	function kindLabel(kind: string): string {
		return kinds.find((k) => k.kind === kind)?.label ?? kind;
	}

	/** One line on what the channel hears; empty when it hears everything, always. */
	function policySummary(channel: Channel): string {
		const p = channel.policy;
		if (!p) return '';
		const parts: string[] = [];
		if (p.min_severity === 'critical') parts.push(m.notifications_channels_policy_warning_only());
		else if (p.min_severity === 'warning') parts.push(m.notifications_channels_policy_advisory_up());
		if (!p.notify_resolved) parts.push(m.notifications_channels_policy_no_recoveries());
		if (p.min_interval_secs > 0) {
			parts.push(m.notifications_channels_policy_interval({ minutes: Math.round(p.min_interval_secs / 60) }));
		}
		if (p.quiet_hours) {
			const clock = (min: number) => `${pad2(Math.floor(min / 60))}:${pad2(min % 60)}`;
			parts.push(
				m.notifications_channels_policy_quiet({
					start: clock(p.quiet_hours.start_minute),
					end: clock(p.quiet_hours.end_minute)
				})
			);
		}
		return parts.join(' · ');
	}

	// --- Form -----------------------------------------------------------------

	/** `null`: closed; `'new'`: adding; a channel: editing it. */
	let form = $state<null | 'new' | Channel>(null);
	/** Adding from a copy of this channel (the form is then `'new'`). */
	let copySource = $state<Channel | null>(null);
	let formKey = $state(0);

	function openForm(next: 'new' | Channel, copyOf: Channel | null = null) {
		form = next;
		copySource = copyOf;
		formKey += 1;
	}
	let highlighted = $state<number | null>(null);

	function afterSave(saved: Channel) {
		const index = channels.findIndex((c) => c.id === saved.id);
		if (index === -1) channels = [...channels, saved];
		else channels[index] = saved;
		form = null;
		copySource = null;
		// A brief glow on the row that just changed, so the eye lands on it.
		highlighted = saved.id;
		setTimeout(() => (highlighted = null), 1800);
	}

	// --- Per-channel actions --------------------------------------------------

	let busy = $state<{ id: number; action: 'test' | 'delete' | 'toggle' } | null>(null);
	let actionError = $state<{ id: number; cause: unknown } | null>(null);
	let testResult = $state<{ id: number; ok: boolean; message: string; filters: string[] } | null>(null);
	/** Bumped when a Toggle change is refused, so the switch is drawn again from the real state. */
	let toggleNonce = $state(0);

	/** Whether this is the only enabled channel: no alert would reach anyone without it. */
	function isLastEnabled(channel: Channel): boolean {
		return channel.enabled && !allChannels.some((c) => c.id !== channel.id && c.enabled);
	}

	async function toggle(channel: Channel, enabled: boolean) {
		if (!enabled && isLastEnabled(channel) && !confirm(m.w1alerts_channels_last_disable_confirm({ name: channel.name }))) {
			toggleNonce += 1;
			return;
		}
		busy = { id: channel.id, action: 'toggle' };
		actionError = null;
		try {
			// Settings are re-sent: an update without them would empty them. Secrets and
			// the delivery policy are omitted, which keeps what is stored.
			const saved = await updateChannel(channel.id, {
				name: channel.name,
				kind: channel.kind,
				enabled,
				settings: channel.settings
			});
			channels = channels.map((c) => (c.id === saved.id ? saved : c));
		} catch (cause) {
			actionError = { id: channel.id, cause };
			toggleNonce += 1;
		} finally {
			busy = null;
		}
	}

	/** The words for a setting that would hold back a real alert, from the test report. */
	function filterNote(code: string): string {
		switch (code) {
			case 'disabled':
				return m.w1alerts_channels_test_filter_disabled();
			case 'min_severity_warning':
				return m.w1alerts_channels_test_filter_advisory();
			case 'min_severity_critical':
				return m.w1alerts_channels_test_filter_warning();
			case 'quiet_hours':
				return m.w1alerts_channels_test_filter_quiet();
			case 'matcher':
				return m.w1alerts_channels_test_filter_matcher();
			default:
				return code;
		}
	}

	async function sendTest(channel: Channel): Promise<boolean> {
		busy = { id: channel.id, action: 'test' };
		actionError = null;
		testResult = null;
		try {
			const report = await testChannel(channel.id);
			testResult = { id: channel.id, ok: report.ok, message: report.message, filters: report.filters ?? [] };
			// The test updates "last sent" and "last error" on the server.
			channels = await listChannels();
			return report.ok;
		} catch (cause) {
			actionError = { id: channel.id, cause };
			return false;
		} finally {
			busy = null;
		}
	}

	async function remove(channel: Channel) {
		if (isLastEnabled(channel) && !confirm(m.w1alerts_channels_last_delete_confirm({ name: channel.name }))) return;
		busy = { id: channel.id, action: 'delete' };
		actionError = null;
		try {
			await deleteChannel(channel.id);
			channels = channels.filter((c) => c.id !== channel.id);
			if (form !== null && form !== 'new' && form.id === channel.id) form = null;
		} catch (cause) {
			actionError = { id: channel.id, cause };
		} finally {
			busy = null;
		}
	}
</script>

<Panel id="notifications-channels" title={m.notifications_channels_title()} description={m.notifications_channels_description()} padded={false}>
	{#snippet aside()}
		{#if !auth.can('notifications.manage')}
			<Plate tone="ghost" label={auth.readOnlyLabel} />
		{:else if !loading && !error && form === null && allChannels.length > 0}
			<ClickSpark>
				<Button variant="primary" size="sm" onclick={() => openForm('new')}>
					<Plus class="size-4" aria-hidden="true" />
					{m.notifications_channels_add()}
				</Button>
			</ClickSpark>
		{/if}
	{/snippet}

	<div class="px-5 py-4">
		{#if error}
			<ErrorNotice {error} title={m.notifications_channels_error_load()} onretry={() => void load()} />
		{:else if loading}
			<div class="grid gap-2">
				<Skeleton class="h-16 w-full" rows={2} />
			</div>
		{:else}
			{#if form !== null}
				<div class="mb-4">
					<!-- The key forces a fresh form on every opening: no leftover input. -->
					{#key formKey}
						<ChannelForm
							kinds={pickKinds}
							channel={form === 'new' ? undefined : form}
							copyOf={copySource ?? undefined}
							onsaved={afterSave}
							oncancel={() => ((form = null), (copySource = null))}
						/>
					{/key}
				</div>
			{/if}

			{#if allChannels.length === 0}
				{#if !auth.can('notifications.manage')}
					<EmptyState icon={BellRing} title={m.notifications_channels_empty_title()} description={m.notifications_channels_empty_viewer()} />
				{:else if form === null}
					<EmptyState
						icon={BellRing}
						title={m.notifications_channels_empty_title()}
						description={m.notifications_channels_empty_admin()}
					>
						{#snippet action()}
							<ClickSpark>
								<Button variant="primary" onclick={() => openForm('new')}>
									<Plus class="size-4" aria-hidden="true" />
									{m.notifications_channels_add()}
								</Button>
							</ClickSpark>
						{/snippet}
					</EmptyState>
				{/if}
			{:else}
				{#if allChannels.length > 3}
					<label class="relative mb-3 block max-w-sm">
						<span class="sr-only">{m.w1alerts_channels_search_label()}</span>
						<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
						<input type="search" class="input !pl-9" placeholder={m.w1alerts_channels_search_placeholder()} bind:value={search} autocomplete="off" />
					</label>
				{/if}
				{#if shownChannels.length === 0}
					<p class="py-3 text-[0.8125rem] text-ink-2" aria-live="polite">{m.w1alerts_channels_nomatch()}</p>
				{/if}
				<ul class="grid gap-2" role="list">
					{#each shownChannels as channel, i (channel.id)}
						{@const Icon = kindIcon(channel.kind)}
						{@const current = busy?.id === channel.id ? busy.action : null}
						<li
							class={`rise-in rounded-[var(--radius-card)] border px-4 py-3 transition-[background-color,border-color,box-shadow] duration-700 ${highlighted === channel.id ? 'border-signal bg-signal-soft shadow-lift' : 'border-line bg-surface'} ${channel.enabled ? '' : 'ghost-cell'}`}
							style="--rise-delay: {Math.min(i, 8) * 30}ms"
						>
							<div class="flex flex-wrap items-start gap-3">
								<span class={`mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-lg ${channel.enabled ? 'bg-signal-soft text-signal-ink' : 'bg-ghost text-ink-3'}`}>
									<Icon class="size-4" aria-hidden="true" />
								</span>
								<!-- A basis of 14rem: on a phone the actions wrap under the text instead of squeezing it. -->
								<div class="min-w-0 flex-[1_1_14rem]">
									<div class="flex flex-wrap items-center gap-x-2 gap-y-1">
										<span class="truncate font-semibold text-ink">{channel.name}</span>
										<FileManagedBadge kind="channel" id={channel.name} />
										<span class="text-sm text-ink-2">{kindLabel(channel.kind)}</span>
										{#if channel.enabled}
											<Plate tone="signal" label={m.notifications_channels_enabled()} />
										{:else}
											<Plate tone="ghost" label={m.notifications_channels_disabled()} />
										{/if}
									</div>
									<p class="mt-1 text-sm text-ink-2">
										{lastSentParts[0]}<time class="tnum" title={formatDateTime(channel.last_sent_at)}>{formatRelative(channel.last_sent_at)}</time>{lastSentParts[1]}
									</p>
									{#if policySummary(channel)}
										<p class="mt-0.5 text-[0.8125rem] text-ink-2">{policySummary(channel)}</p>
									{/if}
									{#if channel.last_error}
										<ChannelErrorDetail raw={channel.last_error} title={m.notifications_channels_last_error_title()} announce={false} class="mt-2" />
									{/if}
								</div>
								{#if auth.can('notifications.manage')}
									<div class="flex flex-wrap items-center gap-1.5">
										<ClientAssign kind="channel" id={channel.id} />
										{#key `${channel.id}-${toggleNonce}`}
											<Toggle
												id={`channel-toggle-${channel.id}`}
												checked={channel.enabled}
												disabled={current !== null}
												label={channel.enabled
													? m.w1alerts_channels_disable_aria({ name: channel.name })
													: m.w1alerts_channels_enable_aria({ name: channel.name })}
												onchange={(value) => void toggle(channel, value)}
											/>
										{/key}
										<Button variant="ghost" size="sm" loading={current === 'test'} disabled={current !== null} onclick={() => sendTest(channel)}>
											{m.notifications_channels_send_test()}
										</Button>
										<Button variant="ghost" size="sm" disabled={current !== null} onclick={() => openForm(channel)}>{m.notifications_channels_edit()}</Button>
										<Menu label={m.notifications_channels_more()}>
											{#snippet trigger({ toggle, open })}
												<Button
													variant="ghost"
													size="sm"
													class="!h-10 !w-10 !px-0 sm:!h-7 sm:!w-7"
													aria-haspopup="menu"
													aria-expanded={open}
													aria-label={m.notifications_channels_more()}
													title={m.notifications_channels_more()}
													disabled={current !== null}
													onclick={toggle}
												>
													<EllipsisVertical class="size-3.5" aria-hidden="true" />
												</Button>
											{/snippet}
											{#snippet children({ close })}
												<div class="grid gap-1">
												<Button
													variant="ghost"
													size="sm"
													class="!justify-start"
													onclick={() => {
														close();
														openForm('new', channel);
													}}
												>
													{m.w1alerts_channels_duplicate()}
												</Button>
												<Confirm confirmLabel={m.notifications_channels_remove_confirm()} loading={current === 'delete'} disabled={current !== null} onconfirm={() => remove(channel)}>
													{m.notifications_channels_remove()}
												</Confirm>
												</div>
											{/snippet}
										</Menu>
									</div>
								{/if}
							</div>

							<div aria-live="polite">
								{#if testResult?.id === channel.id}
									<div class="mt-3 flex flex-wrap items-center gap-2 text-sm">
										<Plate tone={testResult.ok ? 'signal' : 'warning'} label={testResult.ok ? m.notifications_channels_test_ok() : m.notifications_channels_test_failed()} />
										<span class="min-w-0 break-words text-ink-2">{testResult.message}</span>
									</div>
									{#if testResult.ok}
										<div class="mt-2 text-[0.8125rem] text-ink-2">
											{#if testResult.filters.length === 0}
												<p>{m.w1alerts_channels_test_filter_none()}</p>
											{:else}
												<p class="font-medium text-ink">{m.w1alerts_channels_test_filter_title()}</p>
												<ul class="mt-0.5 list-disc pl-5">
													{#each testResult.filters as code (code)}
														<li>{filterNote(code)}</li>
													{/each}
												</ul>
											{/if}
										</div>
									{/if}
								{/if}
								{#if actionError?.id === channel.id}
									{@const failure = toApiError(actionError.cause)}
									<!-- No httpStatus: it is DumbMonit's own answer, not the service's; the status is read from the message. -->
									<ChannelErrorDetail raw={failure.message} title={m.notifications_channels_error_action()} class="mt-3" />
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</div>
</Panel>
