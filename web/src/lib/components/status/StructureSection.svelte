<script lang="ts">
	/**
	 * The "services" section of the status page editor: groups and monitors placed by hand
	 * (reordered by dragging a grip or with the Move up / Move down buttons), monitor
	 * folders ticked as a whole, and the public names of their members.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { tick } from 'svelte';
	import { ArrowDown, ArrowUp, GripVertical, Plus, Trash2, X } from 'lucide-svelte';
	import type { Target } from '#lib/api/index.js';
	import { Button, Combobox, Disclosure, type ComboOption } from '#lib/ui/index.js';
	import GroupPicker from './GroupPicker.svelte';
	import {
		addGroup,
		addItem,
		dropGroup,
		dropItem,
		duplicateNames,
		locate,
		moveGroup,
		moveItem,
		newGroupKey,
		removeGroup,
		removeItem,
		renameGroup,
		setItemLabel,
		type DraftGroup
	} from './structure';

	interface Props {
		targets: Target[];
		/** Groups and monitors placed by hand, in order. */
		structure: DraftGroup[];
		/** Monitor folders ticked as a whole (dynamic). */
		folders: string[];
		/** Public names of the members of ticked folders. */
		folderLabels: Record<number, string>;
		/** Ids of the monitors placed by hand. */
		placed: Set<number>;
		/** Members of ticked folders that are not placed by hand. */
		viaFolders: Target[];
		saving: boolean;
		idPrefix: string;
		open: boolean;
	}

	let {
		targets,
		structure = $bindable(),
		folders = $bindable(),
		folderLabels = $bindable(),
		placed,
		viaFolders,
		saving,
		idPrefix,
		open = $bindable()
	}: Props = $props();

	const SUBHEAD = 'text-sm font-semibold text-ink';
	const HELP = 'text-[0.8125rem] text-ink-2';
	const ICON_BTN =
		'inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink disabled:opacity-40 disabled:hover:bg-transparent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ink';

	const targetById = $derived(new Map(targets.map((t) => [t.id, t])));
	const duplicates = $derived(duplicateNames(structure));
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
</script>

			<Disclosure
				title={m.status_editor_structure_title()}
				summary={placed.size + viaFolders.length > 0 ? m.status_form_services_count({ count: placed.size + viaFolders.length }) : undefined}
				bind:open
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
