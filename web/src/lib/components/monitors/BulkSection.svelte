<script lang="ts">
	/**
	 * Bulk actions of the monitor list (administrators): the floating bar of the selection mode,
	 * and what follows an action: the relay hints, the CVE toast with Undo, the partial-failure report.
	 * The selection itself is the list's: `selecting` and `selected` are bound to it.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import {
		ApiError,
		createSilence,
		deleteTarget,
		updateTarget,
		type Target,
		type TargetId
	} from '#lib/api/index.js';
	import { quickSilencePayload } from '#lib/components/alerts/helpers.js';
	import { setCveScanTargets } from '#lib/api/cves.js';
	import { assignLocation, listLocations } from '#lib/api/locations.js';
	import { listRelays } from '#lib/api/relay.js';
	import { locationForSite } from '#lib/components/devices/relay/relay-site.js';
	import type { MapLocation, RelayAgent } from '#lib/api/types.js';
	import type { TargetBulkReport } from '#lib/api/targetsBulk.js';
	import { cveScanChanges, runBulk } from '#lib/components/devices/listState.js';
	import { alertsStore } from '#lib/stores/alerts.svelte.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ErrorNotice, Toast } from '#lib/ui/index.js';
	import ListBulkBar from './ListBulkBar.svelte';
	import { bulkEditFailures, freeTagKeys } from './bulk-edit.js';
	import { targetToPayload } from './list.js';

	interface Props {
		/** Every monitor, with the list's optimistic overrides applied. */
		targets: Target[];
		/** Ids on screen in an unfolded list: what "select all" and the actions can reach. */
		shownIds: TargetId[];
		folderChoices: string[];
		optionKeysOf: (kind: string) => ReadonlySet<string>;
		selecting: boolean;
		selected: Set<TargetId>;
		/** A short confirmation, shown as a toast by the list. */
		onnotify: (message: string) => void;
		/** A failure shown above the list. */
		onerror: (cause: unknown) => void;
	}

	let {
		targets,
		shownIds,
		folderChoices,
		optionKeysOf,
		selecting = $bindable(),
		selected = $bindable(),
		onnotify,
		onerror
	}: Props = $props();

	function stopSelecting() {
		selecting = false;
		selected = new Set();
	}
	let bulkBusy = $state(false);
	let bulkReport = $state<{ failed: { name: string; error: unknown }[]; total: number } | null>(null);

	const shownSet = $derived(new Set(shownIds));
	/** Selected devices still on screen: a filter change quietly drops the others from the actions. */
	const selectedTargets = $derived(targets.filter((t) => selected.has(t.id) && shownSet.has(t.id)));
	const allShownSelected = $derived(shownIds.length > 0 && shownIds.every((id) => selected.has(id)));

	function toggleAllShown() {
		const next = new Set(selected);
		if (allShownSelected) for (const id of shownIds) next.delete(id);
		else for (const id of shownIds) next.add(id);
		selected = next;
	}

	async function bulk(subjects: Target[], action: (target: Target) => Promise<unknown>) {
		if (bulkBusy || subjects.length === 0) return;
		bulkBusy = true;
		bulkReport = null;
		try {
			const outcome = await runBulk(subjects, action);
			await alertsStore.refresh();
			// What went through leaves the selection; what failed stays for a retry.
			selected = new Set(outcome.failed.map((f) => f.id));
			if (outcome.failed.length > 0) {
				bulkReport = {
					failed: outcome.failed.map((f) => ({ name: f.name, error: f.error })),
					total: subjects.length
				};
			}
		} finally {
			bulkBusy = false;
		}
	}

	const bulkSetEnabled = (enabled: boolean) =>
		bulk(
			selectedTargets.filter((t) => t.enabled !== enabled),
			(t) => updateTarget(t.id, targetToPayload(t, { enabled }))
		);
	const bulkMove = (group: string) =>
		bulk(
			selectedTargets.filter((t) => (t.group_name || '') !== group),
			(t) => updateTarget(t.id, targetToPayload(t, { group_name: group }))
		);
	const bulkDelete = () => bulk(selectedTargets, (t) => deleteTarget(t.id));
	/** "Edit" went out as one request: what it left as it was stays selected, said by name. */
	async function bulkEdited(report: TargetBulkReport) {
		const { ids, failed } = bulkEditFailures(report, selectedTargets);
		await alertsStore.refresh();
		selected = new Set(ids);
		// The server's own reason per monitor, read as the refusal it is (not a network failure).
		const named = failed.map((f) => ({ name: f.name, error: new ApiError(f.error, 400) }));
		bulkReport = named.length > 0 ? { failed: named, total: report.results.length } : null;
		if (failed.length > 0) return;
		onnotify(m.w5monitors_bulkedit_done({ count: report.updated }));
		// Leaves the selection mode so the toast is not hidden behind the bar.
		stopSelecting();
	}

	/** Durations offered by "Silence": a one-off window from now, per monitor of the selection. */
	const silenceChoices = $derived([
		{ secs: 3600, label: m.w1monitors_list_silence_1h() },
		{ secs: 8 * 3600, label: m.w1monitors_list_silence_8h() },
		{ secs: 24 * 3600, label: m.w1monitors_list_silence_1d() },
		{ secs: 7 * 24 * 3600, label: m.w1monitors_list_silence_7d() }
	]);
	async function bulkSilence(secs: number, label: string) {
		const picked = selectedTargets;
		if (bulkBusy || picked.length === 0) return;
		const ends = new Date(Date.now() + secs * 1000);
		// The same window the monitor's own "Snooze" creates, one per monitor.
		await bulk(picked, (t) => createSilence({ ...quickSilencePayload(t, ends), name: m.devices_silence_name({ name: t.name }) }));
		if (bulkReport === null) {
			onnotify(m.w1monitors_list_silence_done({ duration: label }));
			// Leaves the selection mode so the toast is not hidden behind the bar.
			stopSelecting();
		}
	}

	// CVE check in bulk: one request for the whole selection. Turning it off is
	// armed first (second click in the menu, see `ListBulkBar`); the result comes
	// back as a toast with Undo, and the selection mode is left so the toast is
	// not hidden behind the action bar.
	let cveToast = $state<{ key: number; message: string; undo: { ids: TargetId[]; enabled: boolean } | null } | null>(null);
	let cveToastKey = 0;
	const cveUndo = $derived(cveToast?.undo ?? null);

	async function bulkCve(enabled: boolean) {
		const picked = selectedTargets;
		if (bulkBusy || picked.length === 0) return;
		const ids = cveScanChanges(picked, enabled);
		if (ids.length === 0) {
			cveToast = { key: ++cveToastKey, message: m.devices_list_bulk_cve_none(), undo: null };
			return;
		}
		bulkBusy = true;
		bulkReport = null;
		try {
			const { updated } = await setCveScanTargets(ids, enabled);
			await alertsStore.refresh();
			cveToast = {
				key: ++cveToastKey,
				message: (enabled ? m.devices_list_bulk_cve_done_on : m.devices_list_bulk_cve_done_off)({ count: updated }),
				undo: updated > 0 ? { ids, enabled: !enabled } : null
			};
			stopSelecting();
		} catch (error) {
			bulkReport = { failed: picked.map((t) => ({ name: t.name, error })), total: picked.length };
		} finally {
			bulkBusy = false;
		}
	}

	async function undoCve(undo: { ids: TargetId[]; enabled: boolean }) {
		try {
			await setCveScanTargets(undo.ids, undo.enabled);
			await alertsStore.refresh();
		} catch (error) {
			onerror(error);
		}
	}

	// Locations of the world map, offered in bulk once selecting starts.
	let bulkLocations = $state<MapLocation[]>([]);
	$effect(() => {
		if (!selecting) return;
		const controller = new AbortController();
		listLocations(controller.signal)
			.then((found) => (bulkLocations = found.locations))
			.catch(() => (bulkLocations = []));
		return () => controller.abort();
	});
	// Relay agents, offered as "Reached through" in bulk.
	let bulkRelays = $state<RelayAgent[]>([]);
	$effect(() => {
		if (!selecting) return;
		const controller = new AbortController();
		listRelays(controller.signal)
			.then((list) => (bulkRelays = list.filter((r) => r.relay || r.relayed > 0)))
			.catch(() => (bulkRelays = []));
		return () => controller.abort();
	});
	/** Agents push their own metrics: they are never reached through a relay. */
	const relayable = (t: Target) => t.kind !== 'agent' && t.kind !== 'agent_service';
	/** Monitors of the selection left out of "Reached through", said out loud. */
	let relaySkipped = $state(0);
	/** After a move to a relay whose site is a location: offer to place them there. */
	let relayPlace = $state<{ location: MapLocation; ids: TargetId[] } | null>(null);

	async function bulkRelay(agent: RelayAgent | null) {
		const viaAgent = agent ? agent.id : null;
		const subjects = selectedTargets.filter((t) => relayable(t) && t.id !== viaAgent);
		relaySkipped = selectedTargets.length - subjects.length;
		relayPlace = null;
		const moving = subjects.filter((t) => t.via_agent !== viaAgent);
		await bulk(moving, (t) => updateTarget(t.id, targetToPayload(t, { via_agent: viaAgent })));
		const location = locationForSite(bulkLocations, agent?.site);
		if (location && subjects.length > 0) relayPlace = { location, ids: subjects.map((t) => t.id) };
	}

	async function placeRelayed(): Promise<boolean> {
		if (!relayPlace || bulkBusy) return false;
		const { location, ids } = relayPlace;
		bulkBusy = true;
		try {
			await assignLocation(location.id, ids);
			await alertsStore.refresh();
			relayPlace = null;
			return true;
		} catch (error) {
			bulkReport = { failed: [{ name: location.name, error }], total: ids.length };
			return false;
		} finally {
			bulkBusy = false;
		}
	}

	/** One request for the whole selection: the server places them all at once. */
	async function bulkPlace(location: number | null) {
		const subjects = selectedTargets;
		if (bulkBusy || subjects.length === 0) return;
		bulkBusy = true;
		bulkReport = null;
		try {
			await assignLocation(location, subjects.map((t) => t.id));
			await alertsStore.refresh();
			selected = new Set();
		} catch (error) {
			bulkReport = { failed: subjects.map((t) => ({ name: t.name, error })), total: subjects.length };
		} finally {
			bulkBusy = false;
		}
	}
</script>

{#if auth.can('monitors.manage') && selecting}
	<ListBulkBar
		count={selectedTargets.length}
		selection={selectedTargets.length === 0 ? 'none' : allShownSelected ? 'all' : 'some'}
		empty={shownIds.length === 0}
		busy={bulkBusy}
		{silenceChoices}
		{folderChoices}
		locations={bulkLocations}
		relays={bulkRelays}
		targets={selectedTargets}
		tagKeys={freeTagKeys(selectedTargets, optionKeysOf)}
		onedited={(report) => void bulkEdited(report)}
		onselectall={(on) => {
			if (on !== allShownSelected) toggleAllShown();
		}}
		onexit={stopSelecting}
		onsetenabled={(enabled) => void bulkSetEnabled(enabled)}
		onsilence={(secs, label) => void bulkSilence(secs, label)}
		onmove={(folder) => void bulkMove(folder)}
		onplace={(location) => void bulkPlace(location)}
		onrelay={(agent) => void bulkRelay(agent)}
		oncve={(enabled) => void bulkCve(enabled)}
		ondelete={bulkDelete}
	/>
{/if}
{#if relaySkipped > 0 || relayPlace}
	<div class="mb-3 flex flex-wrap items-center gap-2 rounded-[var(--radius-card)] border border-line bg-surface px-3 py-2 text-sm text-ink-2" role="status">
		{#if relaySkipped > 0}
			<p>{m.devices_list_bulk_relay_skipped({ count: relaySkipped })}</p>
		{/if}
		{#if relayPlace}
			<p>{m.devices_list_bulk_relay_place({ count: relayPlace.ids.length, location: relayPlace.location.name })}</p>
			<Button size="sm" variant="secondary" loading={bulkBusy} onclick={() => placeRelayed()}>{m.devices_list_bulk_relay_place_apply()}</Button>
		{/if}
		<Button size="sm" variant="ghost" onclick={() => { relaySkipped = 0; relayPlace = null; }}>{m.devices_list_bulk_dismiss()}</Button>
	</div>
{/if}
{#if cveToast}
	{#key cveToast.key}
		<Toast
			message={cveToast.message}
			actionLabel={cveUndo ? m.devices_list_bulk_cve_undo() : undefined}
			onaction={cveUndo ? () => void undoCve(cveUndo) : undefined}
			onclose={() => (cveToast = null)}
			durationMs={8000}
		/>
	{/key}
{/if}
{#if bulkReport}
	<div class="mb-3">
		<ErrorNotice error={bulkReport.failed[0].error} title={m.devices_list_bulk_partial_title({ failed: bulkReport.failed.length, total: bulkReport.total })}>
			<p class="text-sm text-ink-2">{m.devices_list_bulk_partial_names({ names: bulkReport.failed.map((f) => f.name).join(', ') })}</p>
			<Button size="sm" variant="ghost" class="mt-1" onclick={() => (bulkReport = null)}>{m.devices_list_bulk_dismiss()}</Button>
		</ErrorNotice>
	</div>
{/if}
