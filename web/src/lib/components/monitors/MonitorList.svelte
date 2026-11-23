<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * Monitors › List — the first tab of /targets (the other tabs are the map, the logs and security).
	 *
	 * One sober toolbar, at the size and in the manner of the Alerts list
	 * (`ListToolbar`): search and Filter on the left; the Compact/Tiles switch,
	 * a quiet "Create folder" and "Select" on the right. "Add a monitor", the
	 * one primary, sits in the page header (`routes/targets`). The filters, the
	 * sort and the tags fold behind "Filter" (`ListFilters`). Children are
	 * folded inside their parent (`nesting.ts`), in folders drawn as the groups
	 * of the Alerts list; the column names are written once, over the whole
	 * list (`ColumnHeader`). Selecting shows `ListBulkBar`; the manual order
	 * lives in `reorder.svelte.ts`, the pure filtering in `list.ts`.
	 * Targets and probe states come from the shared store (polled app-wide by
	 * the root layout, see `alertsStore`); this page only adds the sparklines,
	 * from one batched query refreshed every 30 s.
	 */
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '#lib/navigation.js';
	import {
		probeTarget,
		reorderTargets,
		renameTargetFolder,
		updateTarget,
		type Target,
		type TargetId
	} from '#lib/api/index.js';
	import { displayState } from '#lib/format.js';
	import { tagChip } from '#lib/tag-chip.js';
	import type { RowActionHandlers } from '#lib/components/devices/RowActions.svelte';
	import type {  } from '#lib/api/index.js';
	import type {  } from '#lib/components/Chart.svelte';
	import { countAlertsByTarget } from '#lib/alerts-attention.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import ReviewNotice from '#lib/components/device-form/ReviewNotice.svelte';
	import { Button, EmptyState, ErrorNotice, Skeleton, Toast } from '#lib/ui/index.js';
	import ColumnHeader from '#lib/components/devices/ColumnHeader.svelte';
	import RackList from '#lib/components/devices/RackList.svelte';
	import type { RowSelection } from '#lib/components/devices/RackList.svelte';
	import FolderHeader from '#lib/components/devices/FolderHeader.svelte';
	import { loadRackView, saveRackView, type RackView } from '#lib/components/devices/view.js';
	import { buildRack, type RackRow } from '#lib/components/devices/rack.js';
	import {
		foldRack,
		inferHosts,
		loadOpenChoices,
		markContext,
		nestSorted,
		saveOpenChoices,
		withAncestors
	} from '#lib/components/devices/nesting.js';
	import {
		SORT_KEYS,
		availabilityOf,
		freeTags,
		loadListQuery,
		saveListQuery,
		parseListQuery,
		serializeListQuery,
		sortTargets,
		splitDisabled,
		tagCounts,
		type Segment,
		type SortDir,
		type SortKey
	} from '#lib/components/devices/listState.js';
	import {
		buildFolders,
		knownFolders,
		compareFolderSections,
		rootOf,
		worstState
	} from '#lib/components/devices/folders.js';
	import { ChevronRight, PowerOff } from 'lucide-svelte';
	import { listLocations } from '#lib/api/locations.js';
	import type { MapLocation } from '#lib/api/types.js';
	import ListToolbar from './ListToolbar.svelte';
	import ListFilters from './ListFilters.svelte';
	import ListBulkBar from './ListBulkBar.svelte';
	import ListEmpty from './ListEmpty.svelte';
	import BulkSection from './BulkSection.svelte';
	import { ListDecor } from './decor.svelte.js';
	import { ListReorder } from './reorder.svelte.js';
	import type {  } from '#lib/api/targetsBulk.js';
	import {
		countActiveFilters,
		countSegments,
		kindChoices,
		matchCount,
		narrowTargets,
		restoreOverrides,
		targetToPayload,
		toggled,
		troubleCount,
		visibleIds,
		withOverrides
	} from './list.js';

	const targets = $derived(alertsStore.targets);
	const probes = $derived(alertsStore.probes);
	/** One pass over the shared alert list, not a request per monitor. */
	const alertCounts = $derived(countAlertsByTarget(alertsStore.alerts));
	const decor = new ListDecor(() => targets, () => view);
	const sparklines = $derived(decor.sparklines);
	const river = $derived(decor.river);
	const security = $derived(decor.security);
	const kindLabels = $derived(decor.kindLabels);
	/** Option keys per device type: what is not an option of its type is a free tag. */
	const optionKeys = $derived(decor.optionKeys);
	const NO_KEYS: ReadonlySet<string> = new Set();
	const optionKeysOf = (kind: string) => optionKeys.get(kind) ?? NO_KEYS;
	const firstLoad = $derived(alertsStore.loading && targets.length === 0);
	const pageError = $derived(!alertsStore.available ? alertsStore.lastError : null);

	// Filters, sort and layout live in the URL (`?q=&state=&kind=&sort=&dir=&view=`):
	// a filtered view is linkable and survives a reload. The URL is read once
	// here, then kept in step in both directions by the two effects below.
	// Opened with no parameter at all, the list comes back as this viewer left it
	// (view, filters and sort, never the search text); `saveListQuery` below keeps it.
	const initial = untrack(() =>
		page.url.search === '' ? (loadListQuery() ?? parseListQuery(page.url.searchParams)) : parseListQuery(page.url.searchParams)
	);
	let search = $state(initial.search);
	let segment = $state<Segment>(initial.segment);
	let kind = $state(initial.kind);
	let tagFilter = $state<string[]>(initial.tags);
	let site = $state(initial.site);
	let sort = $state<SortKey>(initial.sort);
	let sortDir = $state<SortDir>(initial.dir);

	/** Compact or tiles: remembered per viewer in this browser; the URL wins when it names one. */
	let view = $state<RackView>(initial.view ?? loadRackView());
	function setView(next: RackView) {
		view = next;
		saveRackView(next);
	}
	const names = $derived(new Map(targets.map((t) => [t.id, t.name])));

	const urlSearch = $derived(
		serializeListQuery({
			search,
			segment,
			kind,
			tags: tagFilter,
			site,
			sort,
			dir: sortDir,
			view: view === 'compact' ? null : view
		})
	);
	$effect(() => {
		saveListQuery({ search: '', segment, kind, tags: tagFilter, site, sort, dir: sortDir, view: view === 'compact' ? null : view });
	});
	// Writes still in flight: when one lands, the URL differs from a newer
	// state for a moment, which must not be mistaken for an outside navigation.
	const inflight = new Set<string>();
	$effect(() => {
		const want = urlSearch;
		if (want === untrack(() => page.url.search)) return;
		inflight.add(want);
		void goto(`${untrack(() => page.url.pathname)}${want}`, {
			replace: true,
			reset: false
		}).finally(() => inflight.delete(want));
	});
	// The first run only sees the URL this page was opened on: when the state was
	// restored from this browser (no parameter in the URL), it must not be undone.
	let firstSync = true;
	$effect(() => {
		const have = page.url.search;
		if (firstSync) {
			firstSync = false;
			return;
		}
		if (have === untrack(() => urlSearch) || inflight.has(have)) return;
		const next = parseListQuery(page.url.searchParams);
		search = next.search;
		segment = next.segment;
		kind = next.kind;
		tagFilter = next.tags;
		site = next.site;
		sort = next.sort;
		sortDir = next.dir;
		view = next.view ?? loadRackView();
	});

	const stateOf = (target: Target) => displayState(target, probes.get(target.id));

	// --- Optimistic overrides for a reorder or a folder move/rename/delete:
	// applied on top of the store's `targets` until a refresh confirms them,
	// or rolled back on failure. `position` is only ever compared within one
	// reorder scope, so a sibling-local index is all an override needs. ---
	let positionOverride = $state<Map<TargetId, number>>(new Map());
	let groupOverride = $state<Map<TargetId, string>>(new Map());
	const effectiveTargets = $derived(withOverrides(targets, positionOverride, groupOverride));

	/** Kinds offered by the filter: only those a monitor uses (plus the one in the URL), never an empty choice. */
	const kinds = $derived(kindChoices(targets, kind, kindLabels));

	/** Free tags in use (with their device count), offered in the tag filter. */
	const tagChips = $derived(tagCounts(targets, optionKeysOf));

	/** Devices matching the search box, the kind select and the tags, before the segment. */
	const narrowed = $derived(narrowTargets(targets, { search, kind, site, tags: tagFilter }, kindLabels, optionKeysOf));

	const counts = $derived(countSegments(narrowed, stateOf));

	const segments = $derived([
		{ id: 'all' as const, label: m.devices_targets_seg_all(), count: counts.all },
		{ id: 'attention' as const, label: m.devices_targets_seg_attention(), count: counts.attention },
		{ id: 'reporting' as const, label: m.devices_targets_seg_reporting(), count: counts.reporting },
		{ id: 'disabled' as const, label: m.devices_targets_seg_disabled(), count: counts.disabled }
	]);

	const visible = $derived(visibleIds(narrowed, segment, stateOf));

	// A folder just created in-app, with no device in it yet: there is no
	// server-side folder entity (a folder is only `Target.group_name`), so an
	// empty one exists only for this session — it is slotted in alongside the
	// real sections, in the same order, until a device is dropped into it.
	let pendingEmptyFolders = $state<Set<string>>(new Set());
	const filtered = $derived(
		search.trim() !== '' || segment !== 'all' || kind !== '' || tagFilter.length > 0
	);
	const sorted = $derived(sort !== 'default');
	/** What "Filter" hides that is not at its default: the count on the button. */
	const activeFilters = $derived(countActiveFilters({ segment, kind, site, tags: tagFilter, sorted }));
	let filtersOpen = $state(false);
	/** Reordering hit-tests rows top to bottom: offered in the compact layout only, on the unfiltered rack order. */
	const canReorder = $derived(auth.can('monitors.manage') && !filtered && !sorted && view === 'compact');
	// Disabled devices leave the rack proper for a collapsible section at the bottom.
	const split = $derived(splitDisabled(targets, stateOf));
	const activeVisible = $derived(new Set([...visible].filter((id) => split.active.has(id))));
	const disabledVisible = $derived(new Set([...visible].filter((id) => split.disabled.has(id))));
	// In the table, a sub-monitor that matches the filters is shown inside its
	// parents, brought along as context (not counted, not in "select all").
	// Unfiltered, nothing is added; the tiles stay flat.
	const nested = $derived(view === 'compact');
	// A database or a web check aimed at a server of the list goes inside it,
	// for display only (`inferHosts`): saves and bulk actions keep reading
	// `effectiveTargets`, the order and the tree on screen read `shownTargets`.
	const hosted = $derived(nested ? inferHosts(effectiveTargets) : { targets: effectiveTargets, inferred: new Set<TargetId>() });
	const shownTargets = $derived(hosted.targets);
	const activeListed = $derived(nested ? withAncestors(shownTargets, activeVisible, split.active) : activeVisible);
	const disabledListed = $derived(nested ? withAncestors(shownTargets, disabledVisible, split.disabled) : disabledVisible);
	const nest = (rows: RackRow[]): RackRow[] => (nested ? nestSorted(rows) : rows);
	const kindText = (value: string) => kindLabels.get(value) ?? value;
	/** Availability over the last day, from the bars of the list; `null` without data. */
	const availabilityFor = (target: Target) => availabilityOf(river.get(target.id));
	const folderSections = $derived.by(() => {
		if (sorted) {
			const rows = markContext(
				nest(
					sortTargets(
						shownTargets.filter((t) => activeListed.has(t.id)),
						stateOf,
						kindText,
						sort as Exclude<SortKey, 'default'>,
						sortDir,
						availabilityFor
					)
				),
				activeVisible
			);
			return rows.length > 0 ? [{ key: '', label: '', rows }] : [];
		}
		const base = buildFolders(shownTargets, stateOf, activeListed, hosted.inferred).map((section) => ({
			...section,
			rows: markContext(section.rows, activeVisible)
		}));
		if (filtered || pendingEmptyFolders.size === 0) return base;
		const extra = [...pendingEmptyFolders]
			.filter((key) => !base.some((s) => s.key === key))
			.map((key) => ({ key, label: key, rows: [] }));
		if (extra.length === 0) return base;
		return [...base, ...extra].sort(compareFolderSections);
	});
	const disabledRows = $derived.by((): RackRow[] =>
		markContext(
			sorted
				? nest(
						sortTargets(
							shownTargets.filter((t) => disabledListed.has(t.id)),
							stateOf,
							kindText,
							sort as Exclude<SortKey, 'default'>,
							sortDir,
							availabilityFor
						)
					)
				: buildRack(shownTargets, stateOf, disabledListed, hosted.inferred),
			disabledVisible
		)
	);
	const disabledCount = $derived(matchCount(disabledRows));
	let disabledOpen = $state(false);
	/** The "Disabled" segment is about nothing else: no point folding it. */
	const disabledExpanded = $derived(segment === 'disabled' || disabledOpen);
	const totalRows = $derived(folderSections.reduce((n, s) => n + matchCount(s.rows), 0) + disabledCount);
	/** A single, group-less section looks exactly like the old flat list — no header shown. */
	const showFolderHeaders = $derived(folderSections.length > 1 || (folderSections[0]?.key ?? '') !== '');

	// --- Sub-monitors folded inside their parent (compact layout; the tiles
	// stay flat). Unfiltered, a parent opens by itself when something below it
	// needs attention, and a parent opened or closed by hand stays so for this
	// viewer. Filtered, every parent of a match opens; a toggle then lasts
	// until the filters change. ---
	let openChoices = $state<Map<TargetId, boolean>>(untrack(() => loadOpenChoices()));
	let filterOpenChoices = $state<Map<TargetId, boolean>>(new Map());
	const listNarrowed = $derived(filtered || site !== '');
	$effect(() => {
		void urlSearch;
		untrack(() => {
			if (filterOpenChoices.size > 0) filterOpenChoices = new Map();
		});
	});
	function isOpen(id: TargetId, trouble: number): boolean {
		if (listNarrowed) return filterOpenChoices.get(id) ?? true;
		return openChoices.get(id) ?? trouble > 0;
	}
	function toggleOpen(id: TargetId, open: boolean) {
		if (listNarrowed) {
			filterOpenChoices = new Map(filterOpenChoices).set(id, !open);
			return;
		}
		const next = new Map(openChoices).set(id, !open);
		openChoices = next;
		// Monitors deleted since are forgotten rather than kept for ever.
		saveOpenChoices(new Map([...next].filter(([key]) => names.has(key))));
	}
	const fold = (rows: RackRow[]): RackRow[] => (nested ? foldRack(rows, isOpen) : rows);
	/** What is drawn: each section's rows with closed parents' sub-monitors left out. */
	const shownSections = $derived(folderSections.map((section) => ({ ...section, shown: fold(section.rows) })));
	const shownDisabledRows = $derived(fold(disabledRows));

	function clearFilters() {
		search = '';
		segment = 'all';
		kind = '';
		site = '';
		tagFilter = [];
		sort = 'default';
		sortDir = 'asc';
	}

	// "Create folder" and "Select" are quiet buttons beside the view switch;
	// the folder's name is typed in a small popover under the first.

	// Sites (locations of the world map): a badge on each monitor and a filter.
	let siteLocations = $state<MapLocation[]>([]);
	const sites = $derived(new Map(siteLocations.map((l) => [l.id, { code: l.code, name: l.name }])));
	/**
	 * The site badge and column only tell monitors apart with two locations or
	 * more; filtered to one site, every line would carry the same badge. In
	 * both cases the table has no site column at all.
	 */
	const showSite = $derived(site === '' && siteLocations.length >= 2);
	$effect(() => {
		const controller = new AbortController();
		listLocations(controller.signal)
			.then((found) => (siteLocations = found.locations))
			.catch(() => {});
		return () => controller.abort();
	});
	/** The filter is offered once a site exists; a stale `?site=` stays visible so it can be cleared. */
	const showSiteFilter = $derived(siteLocations.length > 0 || site !== '');

	const sortOptions = $derived<{ id: SortKey; label: string }[]>(
		SORT_KEYS.map((id) => ({
			id,
			label: {
				default: m.devices_list_sort_default(),
				name: m.devices_list_sort_name(),
				status: m.devices_list_sort_status(),
				kind: m.devices_list_sort_kind(),
				checked: m.devices_list_sort_checked(),
				availability: m.w1monitors_list_sort_availability(),
				created: m.w1monitors_list_sort_created()
			}[id]
		}))
	);

	/** Free tags of each monitor as chips on its line (the options of its type are not tags). */
	const rowTags = $derived(
		new Map(
			targets.map((target) => [
				target.id,
				freeTags(target.tags, optionKeysOf(target.kind)).map((pair) => {
					const at = pair.indexOf('=');
					return tagChip(pair.slice(0, at), pair.slice(at + 1));
				})
			])
		)
	);

	// --- Quick actions of a row: check now, enable or disable. A failure is shown
	// above the list like any other action's; the result of a check is a toast. ---
	let rowToast = $state<{ key: number; message: string } | null>(null);
	let rowToastKey = 0;
	const rowActions: RowActionHandlers = {
		probe: async (target) => {
			try {
				await probeTarget(target.id);
				actionError = null;
				await alertsStore.refresh();
				rowToast = { key: ++rowToastKey, message: m.w1monitors_row_probe_done({ name: target.name }) };
			} catch (cause) {
				actionError = cause;
			}
		},
		setEnabled: async (target, enabled) => {
			try {
				await updateTarget(target.id, targetToPayload(target, { enabled }));
				actionError = null;
				await alertsStore.refresh();
			} catch (cause) {
				actionError = cause;
			}
		}
	};

	// --- Folders & manual order (admin only, and only while unfiltered: the
	// reorder scope is computed from the full fleet, not the narrowed view) ---

	let collapsedFolders = $state<Set<string>>(new Set());
	function toggleFolder(key: string) {
		collapsedFolders = toggled(collapsedFolders, key);
	}

	// A failed move, folder edit or reorder is shown above the list, in-app
	// (never a native alert); the list itself comes from the shared store and
	// stays as it was, since the optimistic overrides above are rolled back.
	let actionError = $state<unknown>(null);
	let moving = $state(false);

	async function moveToFolder(target: Target, groupName: string) {
		const current = target.group_name || '';
		if (current === groupName || moving) return;
		moving = true;
		const prev = groupOverride.get(target.id);
		const next = new Map(groupOverride);
		next.set(target.id, groupName);
		groupOverride = next;
		try {
			await updateTarget(target.id, targetToPayload(target, { group_name: groupName }));
			actionError = null;
			await alertsStore.refresh();
			const cleared = new Map(groupOverride);
			cleared.delete(target.id);
			groupOverride = cleared;
		} catch (cause) {
			groupOverride = restoreOverrides(groupOverride, new Map([[target.id, prev]]));
			actionError = cause;
		} finally {
			moving = false;
		}
	}

	/** Every root device currently in folder `key` — the only ones whose own `group_name` is the folder's. */
	function rootsInFolder(key: string): Target[] {
		const roots = new Map<TargetId, Target>();
		for (const root of rootOf(effectiveTargets).values()) roots.set(root.id, root);
		return [...roots.values()].filter((r) => (r.group_name || '') === key);
	}

	async function bulkSetGroup(rows: Target[], groupName: string) {
		if (rows.length === 0 || moving) return;
		moving = true;
		const prevValues = new Map(rows.map((t) => [t.id, groupOverride.get(t.id)] as const));
		const next = new Map(groupOverride);
		for (const t of rows) next.set(t.id, groupName);
		groupOverride = next;
		try {
			await Promise.all(rows.map((t) => updateTarget(t.id, targetToPayload(t, { group_name: groupName }))));
			actionError = null;
			await alertsStore.refresh();
			const cleared = new Map(groupOverride);
			for (const t of rows) cleared.delete(t.id);
			groupOverride = cleared;
		} catch (cause) {
			groupOverride = restoreOverrides(groupOverride, prevValues);
			actionError = cause;
		} finally {
			moving = false;
		}
	}

	function renameFolder(oldKey: string, newKey: string) {
		const trimmed = newKey.trim();
		if (!trimmed || trimmed === oldKey) return;
		if (pendingEmptyFolders.has(oldKey)) {
			const next = new Set(pendingEmptyFolders);
			next.delete(oldKey);
			next.add(trimmed);
			pendingEmptyFolders = next;
		}
		const rows = rootsInFolder(oldKey);
		if (rows.length > 0) void renameFolderOnServer(rows, oldKey, trimmed);
	}

	/** One request renames the folder everywhere, status pages that ticked it included. */
	async function renameFolderOnServer(rows: Target[], from: string, to: string) {
		if (moving) return;
		moving = true;
		const prevValues = new Map(rows.map((t) => [t.id, groupOverride.get(t.id)] as const));
		const next = new Map(groupOverride);
		for (const t of rows) next.set(t.id, to);
		groupOverride = next;
		try {
			await renameTargetFolder(from, to);
			actionError = null;
			await alertsStore.refresh();
			const cleared = new Map(groupOverride);
			for (const t of rows) cleared.delete(t.id);
			groupOverride = cleared;
		} catch (cause) {
			groupOverride = restoreOverrides(groupOverride, prevValues);
			actionError = cause;
		} finally {
			moving = false;
		}
	}

	function deleteFolder(key: string) {
		if (pendingEmptyFolders.has(key)) {
			const next = new Set(pendingEmptyFolders);
			next.delete(key);
			pendingEmptyFolders = next;
		}
		const rows = rootsInFolder(key);
		if (rows.length > 0) void bulkSetGroup(rows, '');
	}

	/** "Create folder": an empty folder with nowhere to drop a
	 *  device yet, until the user drags one in (there is no server-side
	 *  folder entity to create — see `pendingEmptyFolders` above). */
	function createFolder(name: string) {
		const trimmed = name.trim();
		if (!trimmed) return;
		const next = new Set(pendingEmptyFolders);
		next.add(trimmed);
		pendingEmptyFolders = next;
		const nextCollapsed = new Set(collapsedFolders);
		nextCollapsed.delete(trimmed);
		collapsedFolders = nextCollapsed;
	}

	// --- Bulk actions (admin only). Selection is a mode, off by default:
	// "Select" shows a checkbox per row and a floating action bar, so turning
	// it on moves nothing; Escape or Done leaves it. The actions loop the
	// existing per-device API one device at a time. Failures do not stop the
	// run; they are reported together and stay selected for a retry. ---

	let selecting = $state(false);
	let selected = $state<Set<TargetId>>(new Set());
	function stopSelecting() {
		selecting = false;
		selected = new Set();
	}
	/**
	 * Ids on screen in an unfolded list, inside open parents: what "select all"
	 * and the bulk actions can reach. A parent shown only as the context of a
	 * matching sub-monitor is left out.
	 */
	const shownIds = $derived.by(() => {
		const ids: TargetId[] = [];
		for (const section of shownSections) {
			if (showFolderHeaders && collapsedFolders.has(section.key)) continue;
			for (const row of section.shown) if (!row.context) ids.push(row.target.id);
		}
		if (disabledExpanded) for (const row of shownDisabledRows) if (!row.context) ids.push(row.target.id);
		return ids;
	});
	const selection = $derived<RowSelection | null>(
		auth.can('monitors.manage') && selecting
			? {
					selected,
					toggle: (id) => {
						selected = toggled(selected, id);
					}
				}
			: null
	);
	const folderChoices = $derived([...new Set([...knownFolders(targets), ...pendingEmptyFolders])]);

	async function persistOrder(order: TargetId[]) {
		const prev = new Map(positionOverride);
		const next = new Map(positionOverride);
		order.forEach((id, i) => next.set(id, i));
		positionOverride = next;
		try {
			await reorderTargets(order);
			actionError = null;
			await alertsStore.refresh();
			const cleared = new Map(positionOverride);
			order.forEach((id) => cleared.delete(id));
			positionOverride = cleared;
		} catch (cause) {
			positionOverride = prev;
			actionError = cause;
		}
	}

	// --- Manual order, by pointer and by keyboard (`reorder.svelte.ts`). ---
	const reorder = new ListReorder({
		enabled: () => canReorder,
		shown: () => shownTargets,
		saved: () => effectiveTargets,
		stateOf,
		persistOrder,
		moveToFolder
	});
	$effect(() => reorder.listen());

	$effect(() => decor.listenStatic());
	$effect(() => decor.listenBars());
	$effect(() => decor.listenSparklines());
</script>

<ReviewNotice />

{#if !firstLoad && !pageError && targets.length > 0}
	<ListToolbar
		bind:search
		bind:filtersOpen
		{activeFilters}
		{view}
		onviewchange={setView}
		{selecting}
		ontoggleselect={() => (selecting ? stopSelecting() : (selecting = true))}
		oncreatefolder={createFolder}
	/>

	{#if filtersOpen}
		<ListFilters
			bind:segment
			bind:kind
			bind:site
			bind:tags={tagFilter}
			bind:sort
			bind:sortDir
			{segments}
			{kinds}
			{siteLocations}
			{showSiteFilter}
			{tagChips}
			{sortOptions}
			{sorted}
			{activeFilters}
			onclear={clearFilters}
		/>
	{/if}
{/if}

{#if actionError}
	<div class="mb-3"><ErrorNotice error={actionError} title={m.devices_targets_err_save()} /></div>
{/if}

{#if pageError}
	<ErrorNotice
		error={pageError}
		title={m.devices_targets_err_load()}
		onretry={() => void alertsStore.refresh()}
	/>
{:else if firstLoad}
	<div class="flex flex-col gap-2" aria-busy="true" aria-label={m.devices_targets_loading()}>
		<Skeleton class="h-14 w-full rounded-[var(--radius-card)]" rows={5} />
	</div>
{:else if targets.length === 0}
	<ListEmpty />
{:else if totalRows === 0}
	<EmptyState title={m.devices_targets_nomatch_title()} description={m.devices_targets_nomatch_desc()}>
		{#snippet action()}
			<Button variant="ghost" onclick={clearFilters}>{m.devices_targets_clear_filters()}</Button>
		{/snippet}
	</EmptyState>
{:else}
	<p class="sr-only" aria-live="polite">{m.devices_targets_shown({ shown: totalRows, total: targets.length })}</p>
	{#if filtered || (sorted && auth.can('monitors.manage'))}
		<!-- One quiet line: what the filters leave, and why dragging is off while sorted. -->
		<p class="mb-2 flex flex-wrap items-center gap-x-3 text-[0.8125rem] text-ink-2">
			{#if filtered}
				<span class="tnum">{m.devices_targets_shown({ shown: totalRows, total: targets.length })}</span>
			{/if}
			{#if sorted && auth.can('monitors.manage')}
				<span class="text-ink-3">{m.devices_list_sort_no_reorder()}</span>
			{/if}
			<button type="button" class="inline-flex min-h-10 items-center text-ink-2 underline hover:text-ink" onclick={clearFilters}>{m.devices_targets_clear_filters()}</button>
		</p>
	{/if}
	<BulkSection
		targets={effectiveTargets}
		{shownIds}
		{folderChoices}
		{optionKeysOf}
		bind:selecting
		bind:selected
		onnotify={(message) => (rowToast = { key: ++rowToastKey, message })}
		onerror={(cause) => (actionError = cause)}
	/>
	{#if rowToast}
		{#key rowToast.key}
			<Toast message={rowToast.message} onclose={() => (rowToast = null)} durationMs={6000} />
		{/key}
	{/if}
	{#if view === 'compact'}
		<!-- The column names, once over every folder, on the grid of the rows. -->
		<ColumnHeader {showSite} selecting={selection !== null} class="-mb-1" />
	{/if}
	<!-- Folders apart, each header close over its own rows (as the groups of the Alerts list). -->
	<div class="flex flex-col gap-5">
		{#each shownSections as section (section.key)}
			{@const collapsed = collapsedFolders.has(section.key)}
			<div class={`flex flex-col ${view === 'compact' ? 'gap-1.5' : 'gap-2'}`}>
				{#if showFolderHeaders}
					<FolderHeader
						label={section.label}
						folderKey={section.key}
						count={matchCount(section.rows)}
						trouble={troubleCount(section.rows)}
						worst={worstState(section.rows)}
						{collapsed}
						ontoggle={() => toggleFolder(section.key)}
						canManage={auth.can('monitors.manage') && !filtered && section.key !== ''}
						busy={moving}
						onrename={(next) => renameFolder(section.key, next)}
						ondelete={() => deleteFolder(section.key)}
						dropActive={canReorder && reorder.dragging !== null}
						dropHover={reorder.dragging !== null && reorder.dropSlot?.kind === 'folder' && reorder.dropSlot.key === section.key}
						onheaderref={(el) => reorder.registerHeader(section.key, el)}
					/>
				{/if}
				{#if !collapsed}
					<RackList
						rows={section.shown}
						{sparklines}
						{kindLabels}
						{sites}
						{showSite}
						reorder={canReorder ? reorder.controls() : null}
						layout={view}
						{names}
						{security}
						{alertCounts}
						{selection}
						{river}
						ontoggle={toggleOpen}
						tags={rowTags}
						{rowActions}
					/>
				{/if}
			</div>
		{/each}
		{#if disabledRows.length > 0}
			<!-- A quiet group, as the folded ones of the Alerts list: same header, ink-2. -->
			<section class={`flex flex-col ${view === 'compact' ? 'gap-1.5' : 'gap-2'}`} aria-label={m.devices_list_disabled_section({ count: disabledCount })}>
				{#if segment === 'disabled'}
					<h2 class="flex min-h-10 items-center gap-2 text-[0.9375rem] font-semibold text-ink-2">
						<PowerOff class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
						{m.devices_list_disabled_section({ count: disabledCount })}
					</h2>
				{:else}
					<button
						type="button"
						class="-ml-2 flex min-h-10 items-center gap-2 self-start rounded-md px-2 text-[0.9375rem] font-semibold text-ink-2 hover:bg-surface-2/60 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal"
						aria-expanded={disabledOpen}
						onclick={() => (disabledOpen = !disabledOpen)}
					>
						<ChevronRight class={`size-4 shrink-0 text-ink-3 transition-transform duration-200 motion-reduce:transition-none ${disabledOpen ? 'rotate-90' : ''}`} aria-hidden="true" />
						<PowerOff class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
						{m.devices_list_disabled_section({ count: disabledCount })}
					</button>
				{/if}
				{#if disabledExpanded}
					<RackList rows={shownDisabledRows} {sparklines} {kindLabels} {sites} {showSite} layout={view} {names} {alertCounts} {selection} {river} ontoggle={toggleOpen} tags={rowTags} {rowActions} />
				{/if}
			</section>
		{/if}
	</div>
	{#if selection}
		<!-- Room under the last row, so the floating bar never covers it. -->
		<div class="h-24" aria-hidden="true"></div>
	{/if}
{/if}

{#if reorder.dragging !== null && reorder.pointerPos}
	<div
		class="pointer-events-none fixed z-50 flex -translate-y-1/2 items-center gap-2 rounded-[var(--radius-card)] border border-line-strong bg-surface px-3 py-2 text-sm font-semibold text-ink shadow-float"
		style={`left: ${reorder.pointerPos.x + 18}px; top: ${reorder.pointerPos.y}px;`}
	>
		{reorder.dragLabel}
		{#if reorder.dropBlockedHint}
			<span class="text-[0.75rem] font-normal text-ink-2">{m.devices_targets_stays_above()}</span>
		{/if}
	</div>
{/if}
<p class="sr-only" aria-live="polite">{reorder.announce}</p>
