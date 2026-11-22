<script lang="ts">
	/**
	 * The floating action bar of the monitors list while selecting: enable,
	 * disable, silence, move to a folder, and (in one menu, being rarer)
	 * location, relay and CVE check, then delete. Each action is handed back to
	 * the list, which runs it over the selection and reports the outcome.
	 * "Edit" opens `BulkEditDialog` (interval, channels, tags, folder in one
	 * request); its verdicts go back to the list through `onedited`.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { Button, Confirm, Menu, SelectionBar } from '#lib/ui/index.js';
	import type { MapLocation, RelayAgent, Target } from '#lib/api/types.js';
	import type { TargetBulkReport } from '#lib/api/targetsBulk.js';
	import { BellOff, EllipsisVertical, FolderInput, MapPin, Power, PowerOff, RadioTower, ShieldCheck, SlidersHorizontal, Trash2 } from 'lucide-svelte';
	import BulkEditDialog from './BulkEditDialog.svelte';

	interface Props {
		/** Selected monitors still on screen: what the actions reach. */
		count: number;
		selection: 'none' | 'some' | 'all';
		/** No row on screen to pick. */
		empty: boolean;
		busy: boolean;
		silenceChoices: { secs: number; label: string }[];
		folderChoices: string[];
		locations: MapLocation[];
		relays: RelayAgent[];
		/** The selected monitors on screen, for "Edit". */
		targets: Target[];
		/** Free tag keys of the selection, offered for removal in "Edit". */
		tagKeys: string[];
		onedited: (report: TargetBulkReport) => void;
		onselectall: (on: boolean) => void;
		onexit: () => void;
		onsetenabled: (enabled: boolean) => void;
		onsilence: (secs: number, label: string) => void;
		onmove: (folder: string) => void;
		onplace: (location: number | null) => void;
		onrelay: (agent: RelayAgent | null) => void;
		oncve: (enabled: boolean) => void;
		ondelete: () => unknown;
	}

	let {
		count,
		selection,
		empty,
		busy,
		silenceChoices,
		folderChoices,
		locations,
		relays,
		targets,
		tagKeys,
		onedited,
		onselectall,
		onexit,
		onsetenabled,
		onsilence,
		onmove,
		onplace,
		onrelay,
		oncve,
		ondelete
	}: Props = $props();

	let folderDraft = $state('');
	let editDialog = $state<BulkEditDialog | null>(null);
	// Turning the CVE check off is armed first (second click in the menu).
	let cveArmed = $state(false);

	// Menu rows of the selection bar.
	const bulkItem = 'flex min-h-10 w-full items-center rounded-lg px-3 text-left text-[0.8125rem] text-ink hover:bg-surface focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal';
	const bulkHeading = 'flex items-center gap-1.5 px-2 pt-1 pb-0.5 text-[0.75rem] font-semibold text-ink-3';
</script>

<SelectionBar
	label={m.devices_list_bulk_aria()}
	countLabel={m.devices_list_selected({ count })}
	selectAllLabel={m.devices_list_select_all()}
	exitLabel={m.devices_list_select_done()}
	{selection}
	{empty}
	{onselectall}
	{onexit}
>
	<Button size="sm" variant="ghost" disabled={busy || count === 0} aria-haspopup="dialog" onclick={() => editDialog?.show()}>
		<SlidersHorizontal class="size-4" aria-hidden="true" />
		{m.w5monitors_bulkedit_open()}
	</Button>
	<Button size="sm" variant="ghost" disabled={busy || count === 0} onclick={() => onsetenabled(true)}>
		<Power class="size-4" aria-hidden="true" />
		{m.devices_list_bulk_enable()}
	</Button>
	<Button size="sm" variant="ghost" disabled={busy || count === 0} onclick={() => onsetenabled(false)}>
		<PowerOff class="size-4" aria-hidden="true" />
		{m.devices_list_bulk_disable()}
	</Button>
	<Menu label={m.w1monitors_list_bulk_silence()} align="left" placement="up">
		{#snippet trigger({ toggle, open })}
			<Button size="sm" variant="ghost" disabled={busy || count === 0} aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
				<BellOff class="size-4" aria-hidden="true" />
				{m.w1monitors_list_bulk_silence()}
			</Button>
		{/snippet}
		{#snippet children({ close })}
			<ul class="flex min-w-48 flex-col gap-0.5">
				{#each silenceChoices as choice (choice.secs)}
					<li>
						<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onsilence(choice.secs, choice.label); }}>
							{choice.label}
						</button>
					</li>
				{/each}
			</ul>
		{/snippet}
	</Menu>
	<Menu label={m.devices_list_bulk_move()} align="left" placement="up">
		{#snippet trigger({ toggle, open })}
			<Button size="sm" variant="ghost" disabled={busy || count === 0} aria-haspopup="menu" aria-expanded={open} onclick={() => { folderDraft = ''; toggle(); }}>
				<FolderInput class="size-4" aria-hidden="true" />
				{m.devices_list_bulk_move()}
			</Button>
		{/snippet}
		{#snippet children({ close })}
			<ul class="flex min-w-48 flex-col gap-0.5">
				<li>
					<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onmove(''); }}>
						{m.devices_list_bulk_no_folder()}
					</button>
				</li>
				{#each folderChoices as folder (folder)}
					<li>
						<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onmove(folder); }}>
							{folder}
						</button>
					</li>
				{/each}
			</ul>
			<form
				class="mt-1 flex items-center gap-1 border-t border-line p-0.5 pt-1.5"
				onsubmit={(e) => {
					e.preventDefault();
					const name = folderDraft.trim();
					if (!name) return;
					close();
					onmove(name);
				}}
			>
				<input class="input !h-10 min-w-0 flex-1 text-[0.8125rem]" bind:value={folderDraft} placeholder={m.devices_list_bulk_folder_placeholder()} aria-label={m.devices_list_bulk_folder_placeholder()} maxlength="80" />
				<Button type="submit" size="sm" variant="secondary" disabled={!folderDraft.trim()}>{m.devices_list_bulk_folder_apply()}</Button>
			</form>
		{/snippet}
	</Menu>
	<!-- Location, relay and CVE check: rarer, so they share one menu. -->
	<Menu label={m.devices_list_bulk_more()} align="left" placement="up">
		{#snippet trigger({ toggle, open })}
			<Button size="sm" variant="ghost" disabled={busy || count === 0} aria-haspopup="menu" aria-expanded={open} onclick={() => { cveArmed = false; toggle(); }}>
				<EllipsisVertical class="size-4" aria-hidden="true" />
				{m.devices_list_bulk_more()}
			</Button>
		{/snippet}
		{#snippet children({ close })}
			<div class="flex w-64 max-w-full flex-col gap-0.5">
				{#if locations.length > 0}
					<p class={bulkHeading}><MapPin class="size-3.5" aria-hidden="true" />{m.devices_list_bulk_location()}</p>
					<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onplace(null); }}>
						{m.devices_list_bulk_location_auto()}
					</button>
					{#each locations as location (location.id)}
						<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onplace(location.id); }}>
							{location.name}
						</button>
					{/each}
				{/if}
				{#if relays.length > 0}
					<p class={`${bulkHeading} ${locations.length > 0 ? 'mt-1 border-t border-line pt-2' : ''}`}><RadioTower class="size-3.5" aria-hidden="true" />{m.devices_list_bulk_relay()}</p>
					<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onrelay(null); }}>
						{m.devices_list_bulk_relay_direct()}
					</button>
					{#each relays as relay (relay.id)}
						<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); onrelay(relay); }}>
							{relay.site ? m.deviceform_form_relay_site({ name: relay.name, site: relay.site }) : relay.name}
						</button>
					{/each}
				{/if}
				<p class={`${bulkHeading} ${locations.length > 0 || relays.length > 0 ? 'mt-1 border-t border-line pt-2' : ''}`}><ShieldCheck class="size-3.5" aria-hidden="true" />{m.devices_list_bulk_cve()}</p>
				<button type="button" role="menuitem" class={bulkItem} onclick={() => { close(); oncve(true); }}>
					{m.devices_list_bulk_cve_on()}
				</button>
				<button
					type="button"
					role="menuitem"
					class={`${bulkItem} ${cveArmed ? 'font-semibold text-warning-ink' : ''}`}
					onclick={() => {
						if (!cveArmed) {
							cveArmed = true;
							return;
						}
						cveArmed = false;
						close();
						oncve(false);
					}}
				>
					{cveArmed ? m.devices_list_bulk_cve_off_confirm({ count }) : m.devices_list_bulk_cve_off()}
				</button>
			</div>
		{/snippet}
	</Menu>
	<span class="flex-1 max-sm:hidden" aria-hidden="true"></span>
	<Confirm confirmLabel={m.devices_list_bulk_delete_confirm({ count })} loading={busy} disabled={busy || count === 0} onconfirm={ondelete}>
		<Trash2 class="size-4" aria-hidden="true" />
		{m.devices_list_bulk_delete()}
	</Confirm>
</SelectionBar>

<BulkEditDialog bind:this={editDialog} {targets} {folderChoices} {tagKeys} ondone={onedited} />
