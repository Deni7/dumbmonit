<script lang="ts">
	/**
	 * The monitors an SLA report covers: a searchable list grouped by folder,
	 * each group with a checkbox for all of its monitors (half-checked when
	 * only some are), "All" and "None" for what the search shows, and one
	 * line saying how many are in. Ticks become the report's `scope`, as short
	 * as it goes (`scopeOf`): every monitor is the empty scope and a whole
	 * group is its name, so monitors added later are covered too. Nothing
	 * ticked has no scope: `empty` turns true and the parent holds the report
	 * back (an empty scope would mean "everything").
	 */
	import { Search } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { listTargets, type Target } from '#lib/api/index.js';
	import {
		UNGROUPED_GROUP,
		emptyScope,
		scopeGroup,
		scopeIds,
		scopeOf,
		type ReportScope,
		type ScopeMonitor
	} from '#lib/api/reports.js';

	interface Props {
		scope: ReportScope;
		/** True while no monitor is ticked. Bindable. */
		empty?: boolean;
		idPrefix: string;
		disabled?: boolean;
	}

	let { scope = $bindable(), empty = $bindable(false), idPrefix, disabled = false }: Props = $props();

	let targets = $state<Target[]>([]);
	let loaded = $state(false);
	$effect(() => {
		const controller = new AbortController();
		listTargets(controller.signal)
			.then((list) => (targets = Array.isArray(list) ? list : []))
			.catch(() => (targets = []))
			.finally(() => (loaded = true));
		return () => controller.abort();
	});

	let query = $state('');

	const monitors = $derived<ScopeMonitor[]>(targets.map((target) => ({ id: target.id, group: scopeGroup(target.group_name) })));
	const selected = $derived(empty ? new Set<number>() : scopeIds(scope, monitors));

	/** Groups in name order (monitors without one last), each with its monitors in name order. */
	const groups = $derived.by(() => {
		const byName = new Map<string, Target[]>();
		for (const target of targets) {
			const group = scopeGroup(target.group_name);
			byName.set(group, [...(byName.get(group) ?? []), target]);
		}
		return [...byName.entries()]
			.sort(([a], [b]) => (a === UNGROUPED_GROUP ? 1 : b === UNGROUPED_GROUP ? -1 : a.localeCompare(b)))
			.map(([name, members]) => ({ name, members: members.sort((a, b) => a.name.localeCompare(b.name)) }));
	});
	const grouped = $derived(groups.length > 1 || (groups.length === 1 && groups[0].name !== UNGROUPED_GROUP));

	const shownGroups = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return groups;
		return groups
			.map((group) => ({
				name: group.name,
				members: group.name.toLowerCase().includes(q)
					? group.members
					: group.members.filter((t) => `${t.name} ${t.address} ${t.kind}`.toLowerCase().includes(q))
			}))
			.filter((group) => group.members.length > 0);
	});
	const shownIds = $derived(shownGroups.flatMap((group) => group.members.map((t) => t.id)));

	function apply(next: Set<number>) {
		const value = scopeOf(next, monitors);
		if (value === null) {
			empty = true;
			scope = emptyScope();
		} else {
			empty = false;
			scope = value;
		}
	}

	function setMany(ids: number[], on: boolean) {
		const next = new Set(selected);
		for (const id of ids) {
			if (on) next.add(id);
			else next.delete(id);
		}
		apply(next);
	}

	const groupLabel = (name: string) => (name === UNGROUPED_GROUP ? m.reports_layout_scope_ungrouped() : name);

	const count = $derived(selected.size);
	const summary = $derived(
		count === 0
			? m.reports_scope_none_selected()
			: count === monitors.length
				? m.reports_scope_all_selected({ total: monitors.length })
				: m.reports_scope_some_selected({ count, total: monitors.length })
	);

	/** A group checkbox shows "some" as the mixed state. */
	function mixed(node: HTMLInputElement, value: boolean) {
		node.indeterminate = value;
		return {
			update(next: boolean) {
				node.indeterminate = next;
			}
		};
	}
</script>

<fieldset class="grid min-w-0 content-start gap-2" {disabled}>
	<legend class="mb-1.5 block text-sm font-semibold text-ink">{m.reports_layout_scope()}</legend>

	<div class="flex flex-wrap items-center gap-2">
		<div class="relative min-w-0 flex-1 basis-48">
			<label for={`${idPrefix}-scope-search`} class="sr-only">{m.reports_layout_scope_search()}</label>
			<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-3" aria-hidden="true" />
			<input
				id={`${idPrefix}-scope-search`}
				type="search"
				class="input !pl-9"
				placeholder={m.reports_layout_scope_search()}
				autocomplete="off"
				spellcheck="false"
				bind:value={query}
			/>
		</div>
		<div class="flex gap-1">
			<button
				type="button"
				class="inline-flex h-10 items-center rounded-lg px-3 text-sm font-semibold text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal disabled:opacity-50"
				aria-label={query.trim() ? m.reports_scope_all_shown_aria() : undefined}
				disabled={shownIds.length === 0}
				onclick={() => setMany(shownIds, true)}
			>
				{m.reports_scope_all()}
			</button>
			<button
				type="button"
				class="inline-flex h-10 items-center rounded-lg px-3 text-sm font-semibold text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal disabled:opacity-50"
				aria-label={query.trim() ? m.reports_scope_none_shown_aria() : undefined}
				disabled={shownIds.length === 0}
				onclick={() => setMany(shownIds, false)}
			>
				{m.reports_scope_none()}
			</button>
		</div>
	</div>

	<p class={`text-[0.8125rem] tnum ${count === 0 && loaded && monitors.length > 0 ? 'font-medium text-warning-ink' : 'text-ink-2'}`} aria-live="polite">
		{loaded ? summary : ''}
	</p>

	<div class="max-h-80 overflow-y-auto rounded-lg border border-line bg-canvas">
		{#if !loaded}
			<p class="px-3 py-4 text-sm text-ink-2">{m.reports_scope_loading()}</p>
		{:else if monitors.length === 0}
			<p class="px-3 py-4 text-sm text-ink-2">{m.reports_scope_no_monitor()}</p>
		{:else if shownGroups.length === 0}
			<p class="px-3 py-4 text-sm text-ink-2">{m.reports_layout_scope_empty()}</p>
		{:else}
			<ul class="divide-y divide-line" role="list">
				{#each shownGroups as group (group.name)}
					{@const ids = group.members.map((t) => t.id)}
					{@const on = ids.filter((id) => selected.has(id)).length}
					<li>
						{#if grouped}
							<label class="flex min-h-10 cursor-pointer items-center gap-3 bg-surface px-3 text-sm font-semibold text-ink hover:bg-surface-2">
								<input
									type="checkbox"
									class="size-4 shrink-0 accent-[var(--c-ink)]"
									checked={on === ids.length}
									use:mixed={on > 0 && on < ids.length}
									onchange={(event) => setMany(ids, event.currentTarget.checked)}
								/>
								<span class="min-w-0 flex-1 truncate">{groupLabel(group.name)}</span>
							</label>
						{/if}
						<ul role="list">
							{#each group.members as target (target.id)}
								<li>
									<label class={`flex min-h-10 cursor-pointer items-center gap-3 pr-3 text-sm text-ink hover:bg-surface-2 ${grouped ? 'pl-9' : 'pl-3'}`}>
										<input
											type="checkbox"
											class="size-4 shrink-0 accent-[var(--c-ink)]"
											checked={selected.has(target.id)}
											onchange={(event) => setMany([target.id], event.currentTarget.checked)}
										/>
										<span class="min-w-0 flex-1 truncate">{target.name}</span>
										<span class="shrink-0 text-[0.75rem] text-ink-3 max-sm:hidden">{target.kind}</span>
									</label>
								</li>
							{/each}
						</ul>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
	{#if grouped}<p class="text-[0.8125rem] text-ink-2">{m.reports_scope_group_help()}</p>{/if}
</fieldset>
