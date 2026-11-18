<script lang="ts">
	/**
	 * The status page editor, in two panes.
	 *
	 * Left, every setting: the page (title, address, description, who may read
	 * it), its groups and monitors — reordered by dragging a grip or with the
	 * Move up / Move down buttons, which reach every place a drag can — its
	 * look (light, accent, logo, links, white label), search engines, and
	 * subscribers and domain. Right, a live preview drawn by the same component
	 * as the public page (`StatusPageBody`) from the unsaved settings, framed as
	 * a browser tab so the tab title and icon (product or white label) show too.
	 * Under 1024 px the two panes become Settings / Preview tabs.
	 *
	 * Save sends the page, its logo, its access, then its services; Cancel and
	 * any navigation away ask first when something is not saved.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { tick, untrack } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { ArrowDown, ArrowUp, Check, GripVertical, Plus, Trash2, X } from 'lucide-svelte';
	import {
		createStatusPage,
		deleteStatusPageLogo,
		getMailServer,
		previewStatusPage,
		setStatusPageAccess,
		setStatusPageItems,
		statusPageLogoUrl,
		toApiError,
		updateStatusPage,
		uploadStatusPageLogo,
		type Channel,
		type MailServer,
		type PublicStatus,
		type StatusPage,
		type StatusPageAccess,
		type StatusPageAccent,
		type StatusPageSeo,
		type StatusPageTheme,
		type Target
	} from '#lib/api/index.js';
	import { withBase } from '#lib/paths.js';
	import { Button, Combobox, Disclosure, ErrorNotice, Field, Plate, Toggle, type ComboOption } from '#lib/ui/index.js';
	import StatusPageBody from './StatusPageBody.svelte';
	import DomainField from './DomainField.svelte';
	import GroupPicker from './GroupPicker.svelte';
	import SeoSection from './SeoSection.svelte';
	import { ACCENTS, accentClass } from './words';
	import { slugify } from './share';
	import { formFingerprint } from './form-state';
	import { effectiveSeo, pageUrl, seoDefaults } from './seo';
	import { PERIOD_CHOICES } from './period';
	import { buildPreview } from './preview';
	import { emailSubscription } from '../settings/mailServer';
	import {
		addGroup,
		addItem,
		dropGroup,
		dropItem,
		duplicateNames,
		itemsFromStructure,
		locate,
		moveGroup,
		moveItem,
		newGroupKey,
		placedIds,
		removeGroup,
		removeItem,
		renameGroup,
		setItemLabel,
		structureFromItems,
		type DraftGroup
	} from './structure';

	interface Props {
		/** `null` creates a page. */
		page: StatusPage | null;
		targets: Target[];
		/** Notification channels; an SMTP one can mail subscribers instead of the email server. */
		channels?: Channel[];
		onsaved: (page: StatusPage) => void;
		oncancel: () => void;
	}

	let { page, targets, channels = [], onsaved, oncancel }: Props = $props();

	// The editor seeds itself once from the page it was opened for; the route
	// re-mounts it for another page.
	const initial = untrack(() => page);
	const idPrefix = untrack(() => (page ? `sp-${page.id}` : 'sp-new'));

	const SLUG_RULE = /^[a-z0-9-]{2,40}$/;

	let title = $state(initial?.title ?? '');
	let slug = $state(initial?.slug ?? '');
	let slugTouched = $state(initial !== null);
	let description = $state(initial?.description ?? '');
	let theme = $state<StatusPageTheme>(initial?.theme ?? 'auto');
	let published = $state(initial?.published ?? false);
	let accessMode = $state<StatusPageAccess>(initial?.access_mode ?? 'public');
	// Typed password; empty keeps the stored one. Never pre-filled: the server only says "has one".
	let accessPassword = $state('');
	let showDays = $state(initial?.show_uptime_days ?? 90);
	let accent = $state<StatusPageAccent>(initial?.accent ?? 'default');
	let showCalendar = $state(initial?.show_calendar ?? false);
	let showLatency = $state(initial?.show_latency ?? false);
	let hideBranding = $state(initial?.hide_branding ?? false);
	let footerText = $state(initial?.footer_text ?? '');
	let homepageUrl = $state(initial?.homepage_url ?? '');
	// Email subscriptions: a switch; the emails go through the email server of
	// the instance, or through an SMTP channel when one is chosen.
	let subscribeEmail = $state(initial?.subscribe_email ?? (initial?.subscribe_channel_id ?? null) !== null);
	let subscribeChannel = $state<number | null>(initial?.subscribe_channel_id ?? null);
	let domain = $state(initial?.domain ?? '');
	let seo = $state<StatusPageSeo>({
		index: false,
		title: '',
		description: '',
		keywords: '',
		canonical: '',
		image: '',
		lang: '',
		site_name: '',
		...initial?.seo
	});
	// Groups and monitors placed by hand, in order. A new page starts with one group to fill.
	let structure = $state<DraftGroup[]>(
		initial && initial.items.length > 0 ? structureFromItems(initial.items) : [{ key: newGroupKey(), name: '', items: [] }]
	);
	// Monitor folders ticked as a whole (dynamic), and the public names of their members.
	let folders = $state<string[]>([...(initial?.groups ?? [])]);
	let folderLabels = $state<Record<number, string>>(
		Object.fromEntries((initial?.group_labels ?? []).map((entry) => [entry.target_id, entry.label]))
	);
	const smtpChannels = $derived(channels.filter((channel) => channel.kind === 'smtp'));
	const chosenChannel = $derived(smtpChannels.find((channel) => channel.id === subscribeChannel) ?? null);

	// Whether the email server is set, to say so under the switch. Unreadable: say nothing.
	let mailServer = $state<MailServer | null>(null);
	$effect(() => {
		const controller = new AbortController();
		getMailServer(controller.signal)
			.then((loaded) => (mailServer = loaded))
			.catch(() => (mailServer = null));
		return () => controller.abort();
	});
	const emailState = $derived(emailSubscription(subscribeEmail, subscribeChannel, mailServer));

	// Logo: what is stored, and what Save will do to it.
	const MAX_LOGO_BYTES = 256 * 1024;
	const LOGO_TYPES = ['image/png', 'image/jpeg', 'image/webp'];
	const hasStoredLogo = Boolean(initial?.logo_type);
	let pendingLogo = $state<string | null>(null);
	let removeLogo = $state(false);
	let logoError = $state<string | null>(null);
	const storedLogoUrl = initial && hasStoredLogo ? statusPageLogoUrl(initial.id, initial.updated_at) : null;
	const logoPreview = $derived(pendingLogo ?? (hasStoredLogo && !removeLogo ? storedLogoUrl : null));

	function pickLogo(event: Event & { currentTarget: HTMLInputElement }) {
		logoError = null;
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = '';
		if (!file) return;
		if (!LOGO_TYPES.includes(file.type)) {
			logoError = m.status_form_logo_type();
			return;
		}
		if (file.size > MAX_LOGO_BYTES) {
			logoError = m.status_form_logo_size({ kib: MAX_LOGO_BYTES / 1024 });
			return;
		}
		const reader = new FileReader();
		reader.onload = () => {
			pendingLogo = typeof reader.result === 'string' ? reader.result : null;
			removeLogo = false;
		};
		reader.onerror = () => (logoError = m.status_form_logo_read());
		reader.readAsDataURL(file);
	}

	function dropLogo() {
		pendingLogo = null;
		removeLogo = true;
		logoError = null;
	}

	// ---------------------------------------------------------------- structure

	const targetById = $derived(new Map(targets.map((t) => [t.id, t])));
	const placed = $derived(placedIds(structure));
	const duplicates = $derived(duplicateNames(structure));
	// Members of ticked folders that are not placed by hand: listed under the groups.
	const viaFolders = $derived(targets.filter((t) => t.group_name && folders.includes(t.group_name) && !placed.has(t.id)));
	const sortedTargets = $derived([...targets].sort((a, b) => a.name.localeCompare(b.name)));
	const serviceOptions = $derived<ComboOption[]>(
		sortedTargets.map((t) => ({ value: String(t.id), label: t.name, hint: t.kind, group: t.group_name }))
	);
	const selectedIds = $derived([...placed].map(String));

	/** Words for a group in labels and announcements. */
	function groupWord(group: DraftGroup): string {
		return group.name.trim() || m.status_editor_group_untitled();
	}
	function targetName(id: number): string {
		return targetById.get(id)?.name ?? String(id);
	}

	// Announced after a move, for screen readers.
	let announcement = $state('');
	function announce(text: string) {
		announcement = '';
		void tick().then(() => (announcement = text));
	}

	/** After a keyboard move the row is redrawn: keep the focus on its button (or the other one at an end). */
	async function refocus(first: string, second: string) {
		await tick();
		const wanted = document.getElementById(first) as HTMLButtonElement | null;
		const target = wanted && !wanted.disabled ? wanted : (document.getElementById(second) as HTMLButtonElement | null);
		target?.focus();
	}

	function stepItem(id: number, delta: -1 | 1) {
		structure = moveItem(structure, id, delta);
		const at = locate(structure, id);
		if (at) announce(m.status_editor_moved({ name: targetName(id), position: at.index + 1, group: groupWord(structure[at.group]) }));
		const up = `${idPrefix}-item-${id}-up`;
		const down = `${idPrefix}-item-${id}-down`;
		void refocus(delta < 0 ? up : down, delta < 0 ? down : up);
	}

	function stepGroup(key: string, delta: -1 | 1) {
		structure = moveGroup(structure, key, delta);
		const index = structure.findIndex((g) => g.key === key);
		if (index >= 0) announce(m.status_editor_group_moved({ name: groupWord(structure[index]), position: index + 1 }));
		const up = `${idPrefix}-group-${key}-up`;
		const down = `${idPrefix}-group-${key}-down`;
		void refocus(delta < 0 ? up : down, delta < 0 ? down : up);
	}

	function toggleTarget(target: Target, on: boolean, groupKey?: string) {
		if (on) {
			const key = groupKey ?? structure[structure.length - 1]?.key;
			if (!key) {
				const fresh = newGroupKey();
				structure = addItem(addGroup(structure, '', fresh), fresh, { target_id: target.id, label: target.name });
			} else {
				structure = addItem(structure, key, { target_id: target.id, label: target.name });
			}
		} else {
			structure = removeItem(structure, target.id);
		}
	}

	async function newGroup() {
		const key = newGroupKey();
		structure = addGroup(structure, '', key);
		await tick();
		document.getElementById(`${idPrefix}-group-${key}-name`)?.focus();
	}

	function toggleFolder(name: string, on: boolean) {
		folders = on ? [...folders.filter((g) => g !== name), name] : folders.filter((g) => g !== name);
	}

	// Drag and drop (mouse): only the grip starts a drag, so text in the
	// inputs can still be selected. The drop line shows where it will land.
	type Dragged = { kind: 'item'; id: number } | { kind: 'group'; key: string };
	type DropAt = { kind: 'item'; key: string; index: number } | { kind: 'group'; index: number };
	let armed = $state<string | null>(null);
	let dragged = $state<Dragged | null>(null);
	let dropAt = $state<DropAt | null>(null);

	function startDrag(event: DragEvent, what: Dragged) {
		dragged = what;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', what.kind === 'item' ? String(what.id) : what.key);
		}
	}
	function endDrag() {
		dragged = null;
		dropAt = null;
		armed = null;
	}
	/** Before or after the hovered element, from where the pointer is. */
	function half(event: DragEvent): 0 | 1 {
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		return event.clientY > box.top + box.height / 2 ? 1 : 0;
	}
	function overItem(event: DragEvent, key: string, index: number) {
		if (dragged?.kind !== 'item') return;
		event.preventDefault();
		event.stopPropagation();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
		dropAt = { kind: 'item', key, index: index + half(event) };
	}
	function overEmptyGroup(event: DragEvent, key: string) {
		if (dragged?.kind !== 'item') return;
		event.preventDefault();
		dropAt = { kind: 'item', key, index: 0 };
	}
	function overGroup(event: DragEvent, index: number) {
		if (dragged?.kind !== 'group') return;
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
		dropAt = { kind: 'group', index: index + half(event) };
	}
	function drop(event: DragEvent) {
		event.preventDefault();
		event.stopPropagation();
		const what = dragged;
		const where = dropAt;
		endDrag();
		if (!what || !where) return;
		if (what.kind === 'item' && where.kind === 'item') {
			structure = dropItem(structure, what.id, where.key, where.index);
			const at = locate(structure, what.id);
			if (at) announce(m.status_editor_moved({ name: targetName(what.id), position: at.index + 1, group: groupWord(structure[at.group]) }));
		} else if (what.kind === 'group' && where.kind === 'group') {
			structure = dropGroup(structure, what.key, where.index);
			const index = structure.findIndex((g) => g.key === what.key);
			if (index >= 0) announce(m.status_editor_group_moved({ name: groupWord(structure[index]), position: index + 1 }));
		}
	}
	const LINE_BEFORE = 'shadow-[inset_0_2px_0_0_var(--c-focus)]';
	const LINE_AFTER = 'shadow-[inset_0_-2px_0_0_var(--c-focus)]';
	function itemLine(key: string, index: number, last: boolean): string {
		if (dropAt?.kind !== 'item' || dropAt.key !== key) return '';
		if (dropAt.index === index) return LINE_BEFORE;
		if (last && dropAt.index === index + 1) return LINE_AFTER;
		return '';
	}
	function groupLine(index: number, last: boolean): string {
		if (dropAt?.kind !== 'group') return '';
		if (dropAt.index === index) return LINE_BEFORE;
		if (last && dropAt.index === index + 1) return LINE_AFTER;
		return '';
	}

	// ------------------------------------------------------------------ preview

	// The saved page as visitors read it, for the states and history of its services.
	let base = $state<PublicStatus | null>(null);
	async function loadBase(signal?: AbortSignal) {
		if (!initial) return;
		try {
			base = await previewStatusPage(initial.id, signal);
		} catch {
			// The preview still draws, without measurements.
		}
	}
	$effect(() => {
		const controller = new AbortController();
		void loadBase(controller.signal);
		return () => controller.abort();
	});

	// The logo the page will have once saved: the new one, none, the stored one,
	// or (MSP mode) the logo of its client.
	const previewLogo = $derived(
		pendingLogo ?? (removeLogo ? null : hasStoredLogo ? storedLogoUrl : (base?.page.logo_url ?? null))
	);
	const previewDoc = $derived(
		buildPreview(
			{
				slug,
				title: title.trim() || m.status_edit_new_title(),
				description,
				theme,
				showDays,
				accent,
				showCalendar,
				showLatency,
				footerText,
				homepageUrl: /^https?:\/\/[^\s/]+/i.test(homepageUrl.trim()) ? homepageUrl : '',
				logoUrl: previewLogo,
				subscribeEmail: emailState !== 'off' && emailState !== 'missing',
				hideBranding,
				groups: structure,
				folders,
				folderLabels
			},
			targets,
			base
		)
	);
	const origin = typeof window === 'undefined' ? '' : window.location.origin;
	const seoUrl = $derived(pageUrl(origin, slug, domain));
	const seoLogo = $derived(initial?.logo_type && origin ? `${origin}/api/public/status/${slug}/logo` : '');
	// What the browser tab of the public page will read.
	const tabTitle = $derived(effectiveSeo(seo, seoDefaults({ name: title.trim(), description, url: seoUrl, logo: seoLogo })).title);
	const tabIcon = $derived(hideBranding ? previewLogo : withBase('/favicon.svg'));
	const previewTheme = $derived(theme === 'dark' ? 'sp-preview-dark' : theme === 'light' ? 'sp-preview-light' : '');
	const serviceNames = $derived(previewDoc.groups.flatMap((g) => g.items.map((i) => i.label)));

	// ------------------------------------------------------------ save / leave

	const fingerprint = $derived(
		formFingerprint({
			title,
			slug,
			description,
			theme,
			published,
			accessMode,
			accessPassword,
			showDays,
			accent,
			showCalendar,
			showLatency,
			hideBranding,
			footerText,
			homepageUrl,
			subscribeEmail,
			subscribeChannel,
			domain,
			seo,
			folders,
			folderLabels,
			// Group keys are the editor's own: only names and order count.
			structure: structure.map((g) => ({ name: g.name, items: g.items })),
			logo: pendingLogo !== null || (removeLogo && hasStoredLogo)
		})
	);
	const openedAs = untrack(() => fingerprint);
	const dirty = $derived(fingerprint !== openedAs);

	let titleError = $state<string | null>(null);
	let slugError = $state<string | null>(null);
	let homepageError = $state<string | null>(null);
	let domainError = $state<string | null>(null);
	let accessError = $state<string | null>(null);
	let saving = $state(false);
	let leaving = false;
	let error = $state<unknown>(null);

	// Leaving the browser tab, or the page inside the app, with unsaved edits asks first.
	$effect(() => {
		if (!dirty || saving) return;
		const warn = (event: BeforeUnloadEvent) => event.preventDefault();
		window.addEventListener('beforeunload', warn);
		return () => window.removeEventListener('beforeunload', warn);
	});
	beforeNavigate((navigation) => {
		if (leaving || !dirty || navigation.type === 'leave') return;
		if (!window.confirm(m.status_editor_leave_confirm())) navigation.cancel();
	});

	// Sections of the settings column; one that holds a refused field opens.
	let openPage = $state(true);
	let openStructure = $state(true);
	let openLook = $state(true);
	let openSeo = $state(false);
	let openConnect = $state(false);
	// Phones and tablets: one pane at a time.
	let pane = $state<'settings' | 'preview'>('settings');

	function onTitleInput() {
		titleError = null;
		if (!slugTouched) slug = slugify(title);
	}

	/** Shows the first refused field: its pane, its section, then the field itself. */
	async function focusFirstError() {
		const order: [string | null, string, () => void][] = [
			[titleError, 'title', () => (openPage = true)],
			[slugError, 'slug', () => (openPage = true)],
			[accessError, 'access-password', () => (openPage = true)],
			[homepageError, 'homepage', () => (openLook = true)],
			[domainError, 'domain', () => (openConnect = true)]
		];
		const first = order.find(([message]) => message);
		if (!first) return;
		pane = 'settings';
		first[2]();
		await tick();
		const field = document.getElementById(`${idPrefix}-${first[1]}`);
		field?.scrollIntoView({ block: 'center' });
		field?.focus({ preventScroll: true });
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		titleError = title.trim() ? null : m.status_form_title_required();
		slugError = SLUG_RULE.test(slug) ? null : m.status_form_slug_rule();
		homepageError = homepageUrl.trim() === '' || /^https?:\/\/[^\s/]+/i.test(homepageUrl.trim()) ? null : m.status_form_homepage_rule();
		// The server has the full rule; this only catches the usual slips early.
		const host = domain.trim().replace(/^https?:\/\//i, '').replace(/\/$/, '');
		domainError = host === '' || /^[a-z0-9.-]+$/i.test(host) ? null : m.status_form_domain_rule();
		const typed = accessPassword.trim();
		accessError =
			accessMode !== 'password'
				? null
				: typed === ''
					? initial?.has_password
						? null
						: m.status_access_password_required()
					: typed.length < 8
						? m.status_access_password_short()
						: null;
		if (titleError || slugError || homepageError || domainError || accessError) {
			void focusFirstError();
			return;
		}

		saving = true;
		try {
			const payload = {
				title: title.trim(),
				slug,
				description: description.trim(),
				theme,
				published,
				show_uptime_days: showDays,
				accent,
				show_calendar: showCalendar,
				show_latency: showLatency,
				hide_branding: hideBranding,
				footer_text: footerText.trim(),
				homepage_url: homepageUrl.trim(),
				subscribe_email: subscribeEmail,
				subscribe_channel_id: subscribeChannel,
				domain: host === '' ? null : host,
				seo,
				groups: folders,
				group_labels: Object.entries(folderLabels)
					.map(([id, label]) => ({ target_id: Number(id), label: label.trim() }))
					.filter((entry) => entry.label !== '' && viaFolders.some((t) => t.id === entry.target_id))
			};
			let saved = page ? await updateStatusPage(page.id, payload) : await createStatusPage(payload);
			if (pendingLogo) {
				await uploadStatusPageLogo(saved.id, pendingLogo);
				saved = { ...saved, logo_type: 'image' };
			} else if (removeLogo && hasStoredLogo) {
				await deleteStatusPageLogo(saved.id);
				saved = { ...saved, logo_type: null };
			}
			// Access has its own route (the password is hashed there); only sent when it changes.
			if (accessMode !== (initial?.access_mode ?? 'public') || typed !== '') {
				saved = await setStatusPageAccess(saved.id, {
					mode: accessMode,
					password: accessMode === 'password' && typed !== '' ? typed : undefined
				});
			}
			const stored = await setStatusPageItems(saved.id, itemsFromStructure(structure));
			leaving = true;
			onsaved({ ...saved, items: stored });
		} catch (cause) {
			// A refused domain (taken, invalid, DumbMonit's own address) belongs under its field.
			const api = toApiError(cause);
			if ((api.status === 400 || api.status === 409) && /domain|host name|IP address|punycode|DumbMonit itself/i.test(api.message)) {
				domainError = api.message;
				void focusFirstError();
			} else {
				error = cause;
			}
		} finally {
			saving = false;
		}
	}

	function onPaneKey(event: KeyboardEvent) {
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		event.preventDefault();
		pane = pane === 'settings' ? 'preview' : 'settings';
		void tick().then(() => document.getElementById(`${idPrefix}-tab-${pane}`)?.focus());
	}

	const SUBHEAD = 'text-sm font-semibold text-ink';
	const HELP = 'text-[0.8125rem] text-ink-2';
	const ICON_BTN =
		'inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink disabled:opacity-40 disabled:hover:bg-transparent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ink';
	const TAB =
		'inline-flex min-h-10 flex-1 items-center justify-center rounded-md px-3 text-sm font-semibold focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ink';
</script>

<form class="grid gap-4" onsubmit={submit} novalidate>
	<!-- Phones and tablets: Settings / Preview, one at a time. -->
	<div class="flex gap-1 rounded-lg border border-line bg-surface p-1 lg:hidden" role="tablist" aria-label={m.status_editor_tabs_label()}>
		{#each [{ id: 'settings', label: m.status_editor_tab_settings() }, { id: 'preview', label: m.status_editor_tab_preview() }] as tab (tab.id)}
			<button
				id="{idPrefix}-tab-{tab.id}"
				type="button"
				role="tab"
				class={`${TAB} ${pane === tab.id ? 'bg-surface-2 text-ink' : 'text-ink-2 hover:text-ink'}`}
				aria-selected={pane === tab.id}
				aria-controls="{idPrefix}-pane-{tab.id}"
				tabindex={pane === tab.id ? 0 : -1}
				onclick={() => (pane = tab.id as 'settings' | 'preview')}
				onkeydown={onPaneKey}
			>
				{tab.label}
			</button>
		{/each}
	</div>

	<div class="grid items-start gap-4 lg:grid-cols-[minmax(20rem,26rem)_minmax(0,1fr)] xl:grid-cols-[28rem_minmax(0,1fr)]">
		<!-- Left: every setting. -->
		<div id="{idPrefix}-pane-settings" class={`grid min-w-0 gap-3 ${pane === 'settings' ? '' : 'max-lg:hidden'}`} aria-labelledby="{idPrefix}-tab-settings">
			<Disclosure title={m.status_edit_section_page()} bind:open={openPage}>
				<fieldset class="grid gap-4" disabled={saving}>
					<legend class="sr-only">{m.status_edit_section_page()}</legend>
					<Field label={m.status_form_title()} for="{idPrefix}-title" error={titleError} required>
						<input
							id="{idPrefix}-title"
							type="text"
							class="input"
							bind:value={title}
							oninput={onTitleInput}
							placeholder={m.status_form_title_placeholder()}
							maxlength="120"
							aria-invalid={titleError ? 'true' : undefined}
						/>
					</Field>
					<Field label={m.status_form_address()} for="{idPrefix}-slug" error={slugError} help={slug ? m.status_form_address_url({ url: `/s/${slug}` }) : m.status_form_address_help()} required>
						<input
							id="{idPrefix}-slug"
							type="text"
							class="input font-mono"
							bind:value={slug}
							oninput={() => {
								slugTouched = true;
								slugError = null;
							}}
							placeholder="home-lab"
							maxlength="40"
							autocomplete="off"
							spellcheck="false"
							aria-invalid={slugError ? 'true' : undefined}
						/>
					</Field>
					<Field label={m.status_form_description()} for="{idPrefix}-description" help={m.status_form_description_help()}>
						<textarea id="{idPrefix}-description" class="input min-h-10" rows="2" bind:value={description} maxlength="1000" placeholder={m.status_form_description_placeholder()}></textarea>
					</Field>
					<div class="flex min-h-10 items-center justify-between gap-3 rounded-lg border border-line px-3 py-1.5">
						<div class="min-w-0">
							<label for="{idPrefix}-published" class="block text-sm font-semibold text-ink">{m.status_form_published()}</label>
							<p class={HELP}>{published ? m.status_form_published_on() : m.status_form_published_off()}</p>
						</div>
						<Toggle id="{idPrefix}-published" bind:checked={published} disabled={saving} label={m.status_form_published()} />
					</div>
					<Field label={m.status_access_label()} for="{idPrefix}-access" help={m.status_access_help()}>
						<select id="{idPrefix}-access" class="input" bind:value={accessMode}>
							<option value="public">{m.status_access_public()}</option>
							<option value="password">{m.status_access_password()}</option>
							<option value="users">{m.status_access_users()}</option>
						</select>
					</Field>
					{#if accessMode === 'password'}
						<Field
							label={m.status_access_password_label()}
							for="{idPrefix}-access-password"
							error={accessError}
							help={initial?.has_password ? m.status_access_password_keep() : m.status_access_password_help()}
						>
							<input
								id="{idPrefix}-access-password"
								type="password"
								class="input"
								bind:value={accessPassword}
								oninput={() => (accessError = null)}
								autocomplete="new-password"
								maxlength="200"
								aria-invalid={accessError ? 'true' : undefined}
							/>
						</Field>
					{/if}
				</fieldset>
			</Disclosure>

			<Disclosure
				title={m.status_editor_structure_title()}
				summary={placed.size + viaFolders.length > 0 ? m.status_form_services_count({ count: placed.size + viaFolders.length }) : undefined}
				bind:open={openStructure}
			>
				{#if sortedTargets.length === 0}
					<p class="ghost-cell rounded-lg border border-dashed border-line px-4 py-6 text-center text-sm text-ink-2">{m.status_form_services_empty()}</p>
				{:else}
					<fieldset class="grid gap-3" disabled={saving}>
						<legend class="sr-only">{m.status_editor_structure_title()}</legend>
						<p class={HELP}>{m.status_editor_structure_help()}</p>

						<ol class="grid gap-3" aria-label={m.status_editor_structure_title()}>
							{#each structure as group, g (group.key)}
								{@const word = groupWord(group)}
								<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
								<li
									class={`rounded-[var(--radius-card)] border border-line bg-canvas-deep ${groupLine(g, g === structure.length - 1)} ${dragged?.kind === 'group' && dragged.key === group.key ? 'opacity-50' : ''}`}
									draggable={armed === `group-${group.key}` ? 'true' : 'false'}
									ondragstart={(event) => startDrag(event, { kind: 'group', key: group.key })}
									ondragend={endDrag}
									ondragover={(event) => overGroup(event, g)}
									ondrop={drop}
								>
									<div class="flex items-center gap-1 border-b border-line px-1.5 py-1.5">
										<span
											class="inline-flex size-8 shrink-0 cursor-grab pointer-coarse:hidden items-center justify-center rounded text-ink-3 hover:text-ink active:cursor-grabbing"
											title={m.status_editor_drag()}
											aria-hidden="true"
											onpointerdown={() => (armed = `group-${group.key}`)}
											onpointerup={() => (armed = null)}
										>
											<GripVertical class="size-4" />
										</span>
										<input
											id="{idPrefix}-group-{group.key}-name"
											type="text"
											class="input min-w-0 flex-1 font-semibold"
											value={group.name}
											oninput={(event) => (structure = renameGroup(structure, group.key, event.currentTarget.value))}
											placeholder={m.status_editor_group_name_placeholder()}
											maxlength="60"
											aria-label={m.status_editor_group_name_aria({ position: g + 1 })}
										/>
										<button id="{idPrefix}-group-{group.key}-up" type="button" class={ICON_BTN} onclick={() => stepGroup(group.key, -1)} disabled={g === 0} aria-label={m.status_editor_group_up_aria({ name: word })} title={m.status_form_up()}><ArrowUp class="size-4" aria-hidden="true" /></button>
										<button id="{idPrefix}-group-{group.key}-down" type="button" class={ICON_BTN} onclick={() => stepGroup(group.key, 1)} disabled={g === structure.length - 1} aria-label={m.status_editor_group_down_aria({ name: word })} title={m.status_form_down()}><ArrowDown class="size-4" aria-hidden="true" /></button>
										<button type="button" class={ICON_BTN} onclick={() => (structure = removeGroup(structure, group.key))} aria-label={m.status_editor_group_remove_aria({ name: word })} title={m.status_form_logo_remove()}><Trash2 class="size-4" aria-hidden="true" /></button>
									</div>
									{#if duplicates.has(group.name.trim())}
										<p class="px-3 pt-2 text-[0.75rem] text-advisory-ink">{m.status_editor_group_duplicate()}</p>
									{/if}

									{#if group.items.length === 0}
										<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
										<p
											class={`m-2 rounded-md border border-dashed px-3 py-3 text-center text-[0.8125rem] text-ink-2 ${dropAt?.kind === 'item' && dropAt.key === group.key ? 'border-focus bg-surface' : 'border-line'}`}
											ondragover={(event) => overEmptyGroup(event, group.key)}
											ondrop={drop}
										>
											{m.status_editor_group_empty()}
										</p>
									{:else}
										<ul class="divide-y divide-line" aria-label={m.status_editor_group_items_aria({ name: word })}>
											{#each group.items as item, i (item.target_id)}
												{@const target = targetById.get(item.target_id)}
												<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
												<li
													class={`flex items-start gap-1 bg-surface px-1.5 py-1.5 ${itemLine(group.key, i, i === group.items.length - 1)} ${dragged?.kind === 'item' && dragged.id === item.target_id ? 'opacity-50' : ''}`}
													draggable={armed === `item-${item.target_id}` ? 'true' : 'false'}
													ondragstart={(event) => {
														event.stopPropagation();
														startDrag(event, { kind: 'item', id: item.target_id });
													}}
													ondragend={endDrag}
													ondragover={(event) => overItem(event, group.key, i)}
													ondrop={drop}
												>
													<span
														class="mt-1 inline-flex size-8 shrink-0 cursor-grab pointer-coarse:hidden items-center justify-center rounded text-ink-3 hover:text-ink active:cursor-grabbing"
														title={m.status_editor_drag()}
														aria-hidden="true"
														onpointerdown={() => (armed = `item-${item.target_id}`)}
														onpointerup={() => (armed = null)}
													>
														<GripVertical class="size-4" />
													</span>
													<div class="grid min-w-0 flex-1 gap-0.5">
														<input
															type="text"
															class="input"
															value={item.label}
															oninput={(event) => (structure = setItemLabel(structure, item.target_id, event.currentTarget.value))}
															placeholder={target?.name ?? ''}
															maxlength="80"
															aria-label={m.status_form_pick_label_aria({ name: targetName(item.target_id) })}
														/>
														<span class="truncate text-[0.75rem] text-ink-3" title={m.status_services_private_hint()}>
															{target ? `${target.name} · ${target.kind}` : targetName(item.target_id)}
														</span>
														{#if !item.label.trim()}
															<span class="text-[0.75rem] text-warning-ink">{m.status_services_public_empty({ name: targetName(item.target_id) })}</span>
														{/if}
													</div>
													<button id="{idPrefix}-item-{item.target_id}-up" type="button" class={ICON_BTN} onclick={() => stepItem(item.target_id, -1)} disabled={g === 0 && i === 0} aria-label={m.status_form_pick_up_aria({ name: targetName(item.target_id) })} title={m.status_form_up()}><ArrowUp class="size-4" aria-hidden="true" /></button>
													<button id="{idPrefix}-item-{item.target_id}-down" type="button" class={ICON_BTN} onclick={() => stepItem(item.target_id, 1)} disabled={g === structure.length - 1 && i === group.items.length - 1} aria-label={m.status_form_pick_down_aria({ name: targetName(item.target_id) })} title={m.status_form_down()}><ArrowDown class="size-4" aria-hidden="true" /></button>
													<button type="button" class={ICON_BTN} onclick={() => (structure = removeItem(structure, item.target_id))} aria-label={m.status_form_pick_remove_aria({ name: targetName(item.target_id) })} title={m.status_form_logo_remove()}><X class="size-4" aria-hidden="true" /></button>
												</li>
											{/each}
										</ul>
									{/if}
									<div class="p-2">
										<Combobox
											id="{idPrefix}-group-{group.key}-add"
											options={serviceOptions}
											selected={selectedIds}
											ontoggle={(value, on) => {
												const target = targetById.get(Number(value));
												if (target) toggleTarget(target, on, group.key);
											}}
											label={m.status_editor_group_add_monitor({ name: word })}
											emptyText={m.status_form_services_no_match()}
											ungroupedLabel={m.status_form_services_no_folder()}
										/>
									</div>
								</li>
							{/each}
						</ol>
						<Button
							variant="secondary"
							class="justify-self-start"
							onclick={() => {
								void newGroup();
							}}
						>
							<Plus class="size-4" aria-hidden="true" />
							{m.status_editor_group_add()}
						</Button>

						<GroupPicker
							id="{idPrefix}-folders"
							{targets}
							groups={folders}
							picked={placed}
							ontogglegroup={toggleFolder}
							ontoggletarget={(target, on) => toggleTarget(target, on)}
							disabled={saving}
						/>
						{#if viaFolders.length > 0}
							<div class="grid gap-1.5">
								<span class={SUBHEAD}>{m.status_services_from_groups()}</span>
								<ul class="divide-y divide-line overflow-hidden rounded-[var(--radius-card)] border border-line">
									{#each viaFolders as target (target.id)}
										<li class="grid gap-1 px-3 py-2">
											<input
												type="text"
												class="input"
												value={folderLabels[target.id] ?? ''}
												oninput={(event) => (folderLabels = { ...folderLabels, [target.id]: event.currentTarget.value })}
												placeholder={target.name}
												maxlength="80"
												aria-label={m.status_form_pick_label_aria({ name: target.name })}
											/>
											<span class="truncate text-[0.75rem] text-ink-3" title={m.status_services_private_hint()}>{target.name} · {target.group_name}</span>
										</li>
									{/each}
								</ul>
							</div>
						{/if}
					</fieldset>
				{/if}
				<p class="sr-only" aria-live="polite">{announcement}</p>
			</Disclosure>

			<Disclosure title={m.status_edit_section_appearance()} bind:open={openLook}>
				<fieldset class="grid gap-5" disabled={saving}>
					<legend class="sr-only">{m.status_edit_section_appearance()}</legend>
					<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-1 xl:grid-cols-2">
						<Field label={m.status_form_theme()} for="{idPrefix}-theme" help={m.status_edit_theme_help()}>
							<select id="{idPrefix}-theme" class="input" bind:value={theme}>
								<option value="auto">{m.status_form_theme_auto()}</option>
								<option value="light">{m.status_form_theme_day()}</option>
								<option value="dark">{m.status_form_theme_night()}</option>
							</select>
						</Field>
						<Field label={m.status_form_history()} for="{idPrefix}-days" help={m.status_form_history_help()}>
							<select id="{idPrefix}-days" class="input" bind:value={showDays}>
								{#each PERIOD_CHOICES as choice (choice)}
									<option value={choice}>{choice === 1 ? m.status_period_24h() : m.status_share_days({ days: choice })}</option>
								{/each}
							</select>
						</Field>
					</div>

					<div class="grid content-start gap-1.5">
						<span class={SUBHEAD} id="{idPrefix}-accent-label">{m.status_form_accent()}</span>
						<div class="flex flex-wrap gap-2" role="radiogroup" aria-labelledby="{idPrefix}-accent-label">
							{#each ACCENTS as option (option.value)}
								<label class={`inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-md border px-2.5 py-1.5 text-sm focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ink ${accent === option.value ? 'border-ink bg-surface-2 text-ink' : 'border-line text-ink-2 hover:text-ink'} ${accentClass(option.value)}`}>
									<input type="radio" class="sr-only" name="{idPrefix}-accent" value={option.value} bind:group={accent} />
									<span class="size-3.5 rounded-full bg-accent" aria-hidden="true"></span>
									{option.label}
								</label>
							{/each}
						</div>
						<p class={HELP}>{m.status_form_accent_help()}</p>
					</div>

					<div class="grid content-start gap-1.5">
						<span class={SUBHEAD} id="{idPrefix}-logo-label">{m.status_form_logo()}</span>
						<div class="flex items-center gap-3">
							<div class="flex size-14 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-dashed border-line-strong bg-canvas-deep">
								{#if logoPreview}
									<img src={logoPreview} alt={m.status_form_logo_preview()} class="size-full object-contain" />
								{:else}
									<span class="text-[0.6875rem] text-ink-3">{m.status_form_logo_none()}</span>
								{/if}
							</div>
							<div class="flex flex-wrap items-center gap-1.5">
								<label class="inline-flex min-h-10 cursor-pointer items-center rounded-md border border-line bg-surface px-3 py-1.5 text-sm font-semibold text-ink hover:bg-surface-2 focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ink">
									{logoPreview ? m.status_form_logo_replace() : m.status_form_logo_upload()}
									<input type="file" accept="image/png,image/jpeg,image/webp" class="sr-only" onchange={pickLogo} aria-labelledby="{idPrefix}-logo-label" />
								</label>
								{#if logoPreview}
									<Button variant="ghost" size="sm" onclick={dropLogo}>{m.status_form_logo_remove()}</Button>
								{/if}
							</div>
						</div>
						<p class={HELP}>{m.status_form_logo_help()}</p>
						{#if logoError}<p class="text-[0.8125rem] text-warning-ink" role="alert">{logoError}</p>{/if}
					</div>

					<Field label={m.status_form_homepage()} for="{idPrefix}-homepage" error={homepageError} help={m.status_form_homepage_help()}>
						<input id="{idPrefix}-homepage" type="url" class="input" bind:value={homepageUrl} maxlength="300" placeholder="https://example.org" oninput={() => (homepageError = null)} />
					</Field>
					<Field label={m.status_form_footer()} for="{idPrefix}-footer" help={m.status_form_footer_help()}>
						<textarea id="{idPrefix}-footer" class="input min-h-10" bind:value={footerText} maxlength="280" rows="2"></textarea>
					</Field>

					<ul class="divide-y divide-line rounded-lg border border-line" aria-label={m.status_edit_appearance_shows()}>
						<li class="flex min-h-12 items-center justify-between gap-4 px-3 py-2">
							<div class="min-w-0">
								<label for="{idPrefix}-calendar" class="block text-sm text-ink">{m.status_form_calendar()}</label>
								<p class={HELP}>{m.status_form_calendar_help()}</p>
							</div>
							<Toggle id="{idPrefix}-calendar" bind:checked={showCalendar} disabled={saving} label={m.status_form_calendar()} />
						</li>
						<li class="flex min-h-12 items-center justify-between gap-4 px-3 py-2">
							<div class="min-w-0">
								<label for="{idPrefix}-latency" class="block text-sm text-ink">{m.status_form_latency()}</label>
								<p class={HELP}>{m.status_form_latency_help()}</p>
							</div>
							<Toggle id="{idPrefix}-latency" bind:checked={showLatency} disabled={saving} label={m.status_form_latency()} />
						</li>
						<li class="flex min-h-12 items-center justify-between gap-4 px-3 py-2">
							<div class="min-w-0">
								<label for="{idPrefix}-branding" class="block text-sm text-ink">{m.status_editor_branding_label()}</label>
								<p class={HELP}>{m.status_editor_branding_help()}</p>
							</div>
							<Toggle id="{idPrefix}-branding" bind:checked={hideBranding} disabled={saving} label={m.status_editor_branding_label()} />
						</li>
					</ul>
				</fieldset>
			</Disclosure>

			{#if published}
				<Disclosure title={m.status_seo_section()} bind:open={openSeo}>
					<SeoSection bare {idPrefix} bind:seo name={title} {description} url={seoUrl} logo={seoLogo} services={serviceNames} disabled={saving} />
				</Disclosure>
			{/if}

			<Disclosure title={m.status_form_section_connect()} bind:open={openConnect}>
				<fieldset class="grid gap-4" disabled={saving}>
					<legend class="sr-only">{m.status_form_section_connect()}</legend>
					<div class="grid gap-3">
						<div class="flex min-h-12 items-start justify-between gap-4">
							<div class="min-w-0">
								<label for="{idPrefix}-subscribe" class="block text-sm text-ink">{m.status_form_subscribe_email()}</label>
								<p class={HELP}>{m.status_form_subscribers_help()}</p>
							</div>
							<Toggle id="{idPrefix}-subscribe" bind:checked={subscribeEmail} disabled={saving} label={m.status_form_subscribe_email()} />
						</div>
						{#if emailState === 'missing'}
							<div class="flex flex-wrap items-start gap-2 rounded-[var(--radius-card)] border border-warning/35 bg-surface p-3 text-sm text-ink-2" role="note">
								<Plate tone="warning" label={m.status_form_subscribe_no_server_plate()} />
								<p class="min-w-0 flex-1">
									{m.status_form_subscribe_no_server()}
									<a class="inline-flex min-h-10 items-center font-semibold text-ink underline decoration-line underline-offset-4 hover:decoration-ink" href={withBase('/settings/notifications#email-server')}>{m.status_form_subscribe_server_link_setup()}</a>
								</p>
							</div>
						{:else if emailState === 'server' && mailServer}
							<p class={HELP}>
								{m.status_form_subscribe_via_server({ from: mailServer.from_address })}
								<a class="inline-flex min-h-10 items-center font-semibold text-ink underline decoration-line underline-offset-4 hover:decoration-ink" href={withBase('/settings/notifications#email-server')}>{m.status_form_subscribe_server_link()}</a>
							</p>
						{:else if emailState === 'channel' && chosenChannel && !chosenChannel.enabled}
							<p class="text-[0.8125rem] text-warning-ink">{m.status_form_subscribe_channel_disabled({ name: chosenChannel.name })}</p>
						{/if}
						{#if subscribeEmail && (smtpChannels.length > 0 || subscribeChannel !== null)}
							<Field label={m.status_form_subscribe_send_with()} for="{idPrefix}-subscribe-channel" help={m.status_form_subscribe_send_with_help()}>
								<select
									id="{idPrefix}-subscribe-channel"
									class="input"
									value={subscribeChannel === null ? '' : String(subscribeChannel)}
									onchange={(event) => (subscribeChannel = event.currentTarget.value === '' ? null : Number(event.currentTarget.value))}
								>
									<option value="">{m.status_form_subscribe_send_with_server()}</option>
									{#each smtpChannels as channel (channel.id)}
										<option value={String(channel.id)}>{channel.enabled ? m.status_form_subscribers_via({ name: channel.name }) : m.status_form_subscribers_via_disabled({ name: channel.name })}</option>
									{/each}
								</select>
							</Field>
						{/if}
					</div>
					<DomainField
						id="{idPrefix}-domain"
						bind:value={domain}
						saved={initial?.domain ?? null}
						error={domainError}
						disabled={saving}
						oninput={() => (domainError = null)}
					/>
				</fieldset>
			</Disclosure>
		</div>

		<!-- Right: the page as visitors will see it, from the unsaved settings. -->
		<section
			id="{idPrefix}-pane-preview"
			class={`min-w-0 lg:sticky lg:top-4 ${pane === 'preview' ? '' : 'max-lg:hidden'}`}
			aria-labelledby="{idPrefix}-preview-title"
		>
			<div class="overflow-hidden rounded-[var(--radius-card)] border border-line-strong bg-surface shadow-float">
				<!-- The browser tab and address the visitor will see: title, icon (product, logo or none). -->
				<div class="flex items-center gap-2 border-b border-line bg-surface-2 px-3 pt-2">
					<div class="flex min-w-0 max-w-[16rem] items-center gap-2 rounded-t-md border border-b-0 border-line bg-surface px-2.5 py-1.5">
						{#if tabIcon}
							<img src={tabIcon} alt="" class="size-4 shrink-0 rounded-sm object-contain" />
						{:else}
							<span class="size-4 shrink-0 rounded-sm border border-dashed border-line-strong" aria-hidden="true"></span>
						{/if}
						<span class="min-w-0 truncate text-[0.75rem] text-ink" title={m.status_editor_preview_tab_aria({ title: tabTitle })}>{tabTitle}</span>
					</div>
					<h2 id="{idPrefix}-preview-title" class="ml-auto shrink-0 pb-1.5 text-[0.75rem] font-semibold text-ink-2">{m.status_editor_preview_label()}</h2>
					{#if dirty}
						<span class="shrink-0 pb-1.5"><Plate tone="advisory" label={m.status_edit_bar_unsaved()} class="whitespace-nowrap" /></span>
					{/if}
				</div>
				<p class="truncate border-b border-line bg-surface px-3 py-1.5 font-mono text-[0.75rem] text-ink-2">{seoUrl || `/s/${slug}`}</p>
				<!-- Shown, never used: links and forms of the preview do nothing. -->
				<div class={`lg:max-h-[calc(100dvh-12rem)] lg:overflow-y-auto ${previewTheme}`} inert>
					<div class={`min-h-full border-t-2 border-accent bg-canvas text-ink ${accentClass(accent)}`}>
						<div class="mx-auto w-full max-w-3xl px-4 pt-5 pb-8 sm:px-6">
							<StatusPageBody status={previewDoc} />
						</div>
					</div>
				</div>
			</div>
			<p class="mt-2 text-[0.8125rem] text-ink-2">
				{initial ? m.status_editor_preview_note() : m.status_editor_preview_note_new()}
				{#if theme === 'auto'}{m.status_editor_preview_theme_auto()}{/if}
			</p>
		</section>
	</div>

	{#if error}
		<ErrorNotice {error} title={page ? m.status_form_save_error() : m.status_form_create_error()} />
	{/if}

	<!-- Save stays in reach: with something to save, the bar sticks to the bottom of the screen (above the phone tab bar). -->
	<div class={dirty || page === null ? 'sticky bottom-[calc(var(--tabbar-h)+var(--safe-bottom)+0.5rem)] z-20 sm:bottom-3' : ''}>
		<div class={`flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[var(--radius-card)] border bg-surface px-4 py-2.5 shadow-float ${dirty ? 'border-line-strong' : 'border-line'}`}>
			<div class="min-w-0 flex-1 max-sm:basis-full" aria-live="polite">
				{#if page === null}
					<span class="text-sm text-ink-2">{m.status_edit_bar_new()}</span>
				{:else if dirty}
					<Plate tone="advisory" label={m.status_edit_bar_unsaved()} class="whitespace-nowrap" />
				{:else}
					<span class="text-sm text-ink-2">{m.status_edit_bar_clean()}</span>
				{/if}
			</div>
			<Button variant="ghost" onclick={oncancel} disabled={saving} class="max-sm:ml-auto">
				<X class="size-4" aria-hidden="true" />
				{m.status_incidents_cancel()}
			</Button>
			<Button type="submit" variant="primary" loading={saving}>
				<Check class="size-4" aria-hidden="true" />
				{page ? m.status_form_save() : m.status_form_create()}
			</Button>
		</div>
	</div>
</form>
