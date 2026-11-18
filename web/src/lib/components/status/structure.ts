/**
 * The layout of a status page as the editor shows it: ordered groups, each
 * with its ordered monitors. Pure functions, so the reordering (drag and drop,
 * or the Move up / Move down buttons) is tested without a browser.
 *
 * The server stores a flat ordered list of items carrying a `group_name`, and
 * the public page shows the groups in the order their first item appears.
 * `structureFromItems` and `itemsFromStructure` convert both ways; an empty
 * group has no item, so it is not saved.
 */

export interface DraftItem {
	target_id: number;
	/** Public name; empty: the monitor's own name. */
	label: string;
}

export interface DraftGroup {
	/** Stable identity for the editor (keyed lists, drag and drop); never saved. */
	key: string;
	/** Heading on the public page; empty: no heading. */
	name: string;
	items: DraftItem[];
}

export interface SavedItem {
	target_id: number;
	label: string;
	group_name: string;
}

let counter = 0;
/** A key no other group of this session has. */
export function newGroupKey(): string {
	counter += 1;
	return `g${counter}`;
}

/** Groups in the order of their first item, items in saved order. */
export function structureFromItems(items: readonly SavedItem[], makeKey: () => string = newGroupKey): DraftGroup[] {
	const groups: DraftGroup[] = [];
	const seen = new Set<number>();
	for (const item of items) {
		if (seen.has(item.target_id)) continue;
		seen.add(item.target_id);
		const name = item.group_name.trim();
		let group = groups.find((g) => g.name === name);
		if (!group) {
			group = { key: makeKey(), name, items: [] };
			groups.push(group);
		}
		group.items.push({ target_id: item.target_id, label: item.label });
	}
	return groups;
}

/** The flat list the server stores: group by group, trimmed. Empty groups vanish. */
export function itemsFromStructure(groups: readonly DraftGroup[]): SavedItem[] {
	const seen = new Set<number>();
	const out: SavedItem[] = [];
	for (const group of groups) {
		for (const item of group.items) {
			if (seen.has(item.target_id)) continue;
			seen.add(item.target_id);
			out.push({ target_id: item.target_id, label: item.label.trim(), group_name: group.name.trim() });
		}
	}
	return out;
}

/** Every monitor placed in a group, once. */
export function placedIds(groups: readonly DraftGroup[]): Set<number> {
	return new Set(groups.flatMap((g) => g.items.map((i) => i.target_id)));
}

/** Where a monitor sits: its group's index and its own index there; `null` if absent. */
export function locate(groups: readonly DraftGroup[], targetId: number): { group: number; index: number } | null {
	for (let group = 0; group < groups.length; group += 1) {
		const index = groups[group].items.findIndex((i) => i.target_id === targetId);
		if (index >= 0) return { group, index };
	}
	return null;
}

function clone(groups: readonly DraftGroup[]): DraftGroup[] {
	return groups.map((g) => ({ ...g, items: [...g.items] }));
}

/** Moves a group one step up (-1) or down (+1); at an end, nothing changes. */
export function moveGroup(groups: readonly DraftGroup[], key: string, delta: -1 | 1): DraftGroup[] {
	const from = groups.findIndex((g) => g.key === key);
	const to = from + delta;
	if (from < 0 || to < 0 || to >= groups.length) return [...groups];
	const next = [...groups];
	[next[from], next[to]] = [next[to], next[from]];
	return next;
}

/**
 * Drops a group so it lands before the group now at `toIndex` (`groups.length`:
 * at the end). Indexes are those of the list before the move.
 */
export function dropGroup(groups: readonly DraftGroup[], key: string, toIndex: number): DraftGroup[] {
	const from = groups.findIndex((g) => g.key === key);
	if (from < 0) return [...groups];
	const next = [...groups];
	const [moved] = next.splice(from, 1);
	const at = Math.max(0, Math.min(from < toIndex ? toIndex - 1 : toIndex, next.length));
	next.splice(at, 0, moved);
	return next;
}

/**
 * Moves a monitor one step up or down. At the edge of its group it crosses
 * into the neighbouring group (last place of the one above, first place of
 * the one below), so the buttons alone can reach every place a drag can.
 */
export function moveItem(groups: readonly DraftGroup[], targetId: number, delta: -1 | 1): DraftGroup[] {
	const at = locate(groups, targetId);
	if (!at) return [...groups];
	const next = clone(groups);
	const items = next[at.group].items;
	const to = at.index + delta;
	if (to >= 0 && to < items.length) {
		[items[at.index], items[to]] = [items[to], items[at.index]];
		return next;
	}
	const neighbour = at.group + delta;
	if (neighbour < 0 || neighbour >= next.length) return next;
	const [moved] = items.splice(at.index, 1);
	if (delta < 0) next[neighbour].items.push(moved);
	else next[neighbour].items.unshift(moved);
	return next;
}

/**
 * Drops a monitor into group `toKey`, before the monitor now at `toIndex`
 * there (`items.length`: at the end). Indexes are those before the move.
 */
export function dropItem(groups: readonly DraftGroup[], targetId: number, toKey: string, toIndex: number): DraftGroup[] {
	const at = locate(groups, targetId);
	const dest = groups.findIndex((g) => g.key === toKey);
	if (!at || dest < 0) return [...groups];
	const next = clone(groups);
	const [moved] = next[at.group].items.splice(at.index, 1);
	let index = toIndex;
	if (at.group === dest && at.index < toIndex) index -= 1;
	index = Math.max(0, Math.min(index, next[dest].items.length));
	next[dest].items.splice(index, 0, moved);
	return next;
}

/** Adds a monitor at the end of a group; one already placed elsewhere moves there. */
export function addItem(groups: readonly DraftGroup[], key: string, item: DraftItem): DraftGroup[] {
	const dest = groups.findIndex((g) => g.key === key);
	if (dest < 0) return [...groups];
	const next = removeItem(groups, item.target_id);
	next[dest].items.push(item);
	return next;
}

export function removeItem(groups: readonly DraftGroup[], targetId: number): DraftGroup[] {
	return groups.map((g) => ({ ...g, items: g.items.filter((i) => i.target_id !== targetId) }));
}

export function setItemLabel(groups: readonly DraftGroup[], targetId: number, label: string): DraftGroup[] {
	return groups.map((g) => ({
		...g,
		items: g.items.map((i) => (i.target_id === targetId ? { ...i, label } : i))
	}));
}

export function addGroup(groups: readonly DraftGroup[], name: string, key: string = newGroupKey()): DraftGroup[] {
	return [...groups, { key, name, items: [] }];
}

export function renameGroup(groups: readonly DraftGroup[], key: string, name: string): DraftGroup[] {
	return groups.map((g) => (g.key === key ? { ...g, name } : g));
}

/** Removes a group and the monitors in it. */
export function removeGroup(groups: readonly DraftGroup[], key: string): DraftGroup[] {
	return groups.filter((g) => g.key !== key);
}

/** Groups whose (trimmed) name another group already has: the public page shows them as one. */
export function duplicateNames(groups: readonly DraftGroup[]): Set<string> {
	const seen = new Set<string>();
	const twice = new Set<string>();
	for (const group of groups) {
		const name = group.name.trim();
		if (seen.has(name)) twice.add(name);
		seen.add(name);
	}
	return twice;
}
