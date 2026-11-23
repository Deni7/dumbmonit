<script lang="ts">
	/**
	 * Settings → My account → My notifications: what this account receives
	 * itself, by email (its address, through the instance email server) or on
	 * its push devices, on top of the team channels. Minimum severity, scope
	 * (everything it sees, its teams' folders, or picked folders and tags), a
	 * minimum delay between repeats, and quiet hours in this browser's time
	 * zone. Alerts its teams do not show never reach it.
	 */
	import { Send } from 'lucide-svelte';
	import type { AlertSeverity, QuietHours } from '#lib/api/index.js';
	import {
		getMyNotifications,
		saveMyNotifications,
		testMyNotifications,
		type MyNotifications,
		type PersonalPrefs,
		type PersonalScope,
		type PersonalTestReport
	} from '#lib/api/accounts.js';
	import { Button, Chip, ErrorNotice, Field, Panel, Plate, Segmented, Skeleton, Toggle } from '#lib/ui/index.js';
	import QuietHoursEditor from '#lib/components/notifications/QuietHoursEditor.svelte';
	import { m } from '#lib/paraglide/messages.js';

	const NOTHING: PersonalPrefs = {
		email: false,
		webpush: false,
		min_severity: 'warning',
		notify_resolved: true,
		scope: 'all',
		folders: [],
		tags: [],
		min_interval_secs: 0,
		quiet_hours: null
	};

	let view = $state<MyNotifications | null>(null);
	/** The choices being edited; saved with "Save". */
	let prefs = $state<PersonalPrefs>(structuredClone(NOTHING));
	let loadError = $state<unknown>(null);
	let saving = $state(false);
	let saveError = $state<unknown>(null);
	let testing = $state(false);
	let report = $state<PersonalTestReport | null>(null);
	let testError = $state<unknown>(null);
	/** Bumped on each load, so the quiet-hours editor is seeded again. */
	let generation = $state(0);

	async function load() {
		loadError = null;
		try {
			const fresh = await getMyNotifications();
			view = fresh;
			prefs = structuredClone(fresh.prefs);
			generation += 1;
		} catch (cause) {
			loadError = cause;
		}
	}

	$effect(() => {
		void load();
	});

	const SEVERITIES: { id: AlertSeverity; label: () => string }[] = [
		{ id: 'info', label: () => m.notifications_form_floor_info() },
		{ id: 'warning', label: () => m.notifications_form_floor_warning() },
		{ id: 'critical', label: () => m.notifications_form_floor_critical() }
	];
	const INTERVALS: { value: number; label: () => string }[] = [
		{ value: 0, label: () => m.notifications_form_no_minimum() },
		{ value: 300, label: () => m.alerts_span_min({ minutes: 5 }) },
		{ value: 900, label: () => m.alerts_span_min({ minutes: 15 }) },
		{ value: 3600, label: () => m.alerts_span_h({ hours: 1 }) },
		{ value: 14400, label: () => m.alerts_span_h({ hours: 4 }) },
		{ value: 86400, label: () => m.alerts_span_h({ hours: 24 }) }
	];
	const intervals = $derived.by(() => {
		const current = prefs.min_interval_secs;
		if (INTERVALS.some((o) => o.value === current)) return INTERVALS;
		return [...INTERVALS, { value: current, label: () => m.notifications_form_interval_current({ seconds: current }) }];
	});
	const SCOPES = $derived<{ value: PersonalScope; label: string }[]>([
		{ value: 'all', label: m.w6accounts_mine_scope_all() },
		{ value: 'teams', label: m.w6accounts_mine_scope_teams() },
		{ value: 'pick', label: m.w6accounts_mine_scope_pick() }
	]);

	const canEmail = $derived(!!view?.email && !!view?.mail_server);
	const emailHelp = $derived.by(() => {
		if (!view) return undefined;
		if (!view.email) return m.w6accounts_mine_email_no_address();
		if (!view.mail_server) return m.w6accounts_mine_email_no_server();
		return m.w6accounts_mine_email_to({ email: view.email });
	});
	const pushHelp = $derived(
		view && view.push_devices > 0 ? m.w6accounts_mine_push_devices({ count: view.push_devices }) : m.w6accounts_mine_push_none()
	);
	const scopeHelp = $derived.by(() => {
		if (!view) return undefined;
		if (prefs.scope === 'teams') {
			return view.teams.length > 0 ? m.w6accounts_mine_scope_teams_help({ teams: view.teams.join(', ') }) : m.w6accounts_mine_scope_no_team();
		}
		if (prefs.scope === 'pick') return m.w6accounts_mine_scope_pick_help();
		return m.w6accounts_mine_scope_all_help();
	});

	// Tags can be many: a filter narrows the chips shown.
	let tagFilter = $state('');
	const shownTags = $derived.by(() => {
		if (!view) return [];
		const query = tagFilter.trim().toLowerCase();
		const chosen = prefs.tags;
		const matches = view.tags.filter((tag) => !query || tag.toLowerCase().includes(query));
		return [...new Set([...chosen, ...matches.slice(0, 40)])];
	});

	function togglePick(list: 'folders' | 'tags', value: string) {
		const current = prefs[list];
		prefs[list] = current.includes(value) ? current.filter((v) => v !== value) : [...current, value];
	}

	function setQuietHours(next: QuietHours | null) {
		// The browser's named zone travels with the window: it follows daylight saving.
		const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
		prefs.quiet_hours = next ? { ...next, timezone: timezone || null } : null;
	}

	async function save(): Promise<boolean> {
		saving = true;
		saveError = null;
		try {
			const saved = await saveMyNotifications($state.snapshot(prefs));
			view = saved;
			prefs = structuredClone(saved.prefs);
			return true;
		} catch (cause) {
			saveError = cause;
			return false;
		} finally {
			saving = false;
		}
	}

	async function test() {
		testing = true;
		testError = null;
		report = null;
		try {
			report = await testMyNotifications();
		} catch (cause) {
			testError = cause;
		} finally {
			testing = false;
		}
	}
</script>

<Panel id="my-notifications" title={m.w6accounts_mine_title()} description={m.w6accounts_mine_description()}>
	{#if loadError}
		<ErrorNotice error={loadError} title={m.w6accounts_mine_load_error()} onretry={() => void load()} />
	{:else if !view}
		<div class="grid gap-2"><Skeleton class="h-16 w-full" rows={3} /></div>
	{:else}
		<div class="grid gap-4">
			<Field label={m.w6accounts_mine_email()} for="mine-email" inline help={emailHelp}>
				<Toggle id="mine-email" checked={prefs.email} disabled={saving || (!canEmail && !prefs.email)} onchange={(value) => (prefs.email = value)} />
			</Field>
			<Field label={m.w6accounts_mine_push()} for="mine-push" inline help={pushHelp}>
				<Toggle id="mine-push" checked={prefs.webpush} disabled={saving} onchange={(value) => (prefs.webpush = value)} />
			</Field>

			{#if prefs.email || prefs.webpush}
				<div class="grid gap-4 sm:grid-cols-2">
					<Field label={m.notifications_form_send()} for="mine-severity">
						<select id="mine-severity" class="input" bind:value={prefs.min_severity} disabled={saving}>
							{#each SEVERITIES as option (option.id)}
								<option value={option.id}>{option.label()}</option>
							{/each}
						</select>
					</Field>
					<Field label={m.notifications_form_interval()} for="mine-interval">
						<select id="mine-interval" class="input" bind:value={prefs.min_interval_secs} disabled={saving}>
							{#each intervals as option (option.value)}
								<option value={option.value}>{option.label()}</option>
							{/each}
						</select>
					</Field>
				</div>
				<Field label={m.notifications_form_resolved()} for="mine-resolved" inline help={m.w6accounts_mine_resolved_help()}>
					<Toggle id="mine-resolved" checked={prefs.notify_resolved} disabled={saving} onchange={(value) => (prefs.notify_resolved = value)} />
				</Field>

				<Segmented
					legend={m.w6accounts_mine_scope()}
					name="mine-scope"
					options={SCOPES}
					value={prefs.scope}
					help={scopeHelp}
					disabled={saving}
					onchange={(next) => (prefs.scope = next as PersonalScope)}
				/>

				{#if prefs.scope === 'pick'}
					<fieldset class="grid gap-3">
						<legend class="mb-1.5 text-sm font-semibold text-ink">{m.w6accounts_mine_folders()}</legend>
						{#if view.folders.length === 0}
							<p class="text-[0.8125rem] text-ink-2">{m.w6accounts_mine_folders_none()}</p>
						{:else}
							<div class="flex flex-wrap gap-2">
								{#each view.folders as folder (folder)}
									<Chip pressed={prefs.folders.includes(folder)} disabled={saving} onclick={() => togglePick('folders', folder)}>
										<span class="max-w-[14rem] truncate" title={folder}>{folder}</span>
									</Chip>
								{/each}
							</div>
						{/if}
					</fieldset>
					<fieldset class="grid gap-3">
						<legend class="mb-1.5 text-sm font-semibold text-ink">{m.w6accounts_mine_tags()}</legend>
						{#if view.tags.length === 0}
							<p class="text-[0.8125rem] text-ink-2">{m.w6accounts_mine_tags_none()}</p>
						{:else}
							<label for="mine-tag-filter" class="sr-only">{m.w6accounts_mine_tags_filter()}</label>
							<input id="mine-tag-filter" type="search" class="input" bind:value={tagFilter} placeholder={m.w6accounts_mine_tags_filter()} autocomplete="off" spellcheck="false" />
							<div class="flex flex-wrap gap-2">
								{#each shownTags as tag (tag)}
									<Chip pressed={prefs.tags.includes(tag)} disabled={saving} onclick={() => togglePick('tags', tag)}>
										<span class="max-w-[14rem] truncate" title={tag}>{tag}</span>
									</Chip>
								{/each}
							</div>
						{/if}
					</fieldset>
				{/if}

				{#key generation}
					<QuietHoursEditor value={prefs.quiet_hours} idPrefix="mine-quiet" disabled={saving} onchange={setQuietHours} />
				{/key}
				<p class="-mt-2 text-[0.8125rem] text-ink-2">{m.w6accounts_mine_quiet_help()}</p>
			{/if}

			{#if saveError}
				<ErrorNotice error={saveError} title={m.w6accounts_mine_save_error()} />
			{/if}
			<div class="flex flex-wrap items-center gap-2">
				<Button variant="primary" loading={saving} onclick={save}>{m.w6accounts_mine_save()}</Button>
				{#if view.prefs.email || view.prefs.webpush}
					<Button variant="secondary" loading={testing} onclick={() => void test()}>
						<Send class="size-4" aria-hidden="true" />
						{m.w6accounts_mine_test()}
					</Button>
				{/if}
			</div>
			{#if testError}
				<ErrorNotice error={testError} title={m.w6accounts_mine_test_error()} />
			{/if}
			<!-- Always in the page, so the outcome of a test is announced when it lands. -->
			<div class="flex flex-wrap gap-2 empty:hidden" aria-live="polite">
				{#if report}
					{#if report.email}
						<!-- A failure carries the server's reason: the plate grows to wrap it on a phone. -->
						<Plate
							class="max-w-full !h-auto min-h-7 py-1 !leading-snug"
							tone={report.email.ok ? 'signal' : 'warning'}
							label={report.email.ok ? m.w6accounts_mine_test_email_ok() : m.w6accounts_mine_test_email_failed({ error: report.email.error ?? '' })}
						/>
					{/if}
					{#if report.push}
						<Plate
							class="max-w-full !h-auto min-h-7 py-1 !leading-snug"
							tone={report.push.ok ? 'signal' : 'warning'}
							label={report.push.ok ? m.w6accounts_mine_test_push_ok() : m.w6accounts_mine_test_push_failed({ error: report.push.error ?? '' })}
						/>
					{/if}
				{/if}
			</div>
		</div>
	{/if}
</Panel>
