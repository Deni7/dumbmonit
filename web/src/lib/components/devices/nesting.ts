/**
 * Sub-monitors folded inside their parent in the monitors table.
 *
 * The relation is `Target.parent_id`: a service an agent discovers (Docker,
 * ZFS, Plakar… `agent_service`), a monitor declared by an agent's Docker
 * labels, or a parent chosen by hand. `buildRack` already lists the children
 * right under their parent, one depth deeper; this module only decides which
 * of those rows are on screen. A parent shows a disclosure with its count of
 * direct sub-monitors; its children appear when it is open.
 *
 * While the list is filtered, a matching sub-monitor brings its parents along
 * (`withAncestors`, flagged `context`) so it is still shown where it lives.
 *
 * Which parents are open is a per-viewer convenience kept in `localStorage`
 * (every access guarded: private windows and blocked storage throw).
 */
import type { Target, TargetId } from '#lib/api/index.js';
// Explicit extension: this module is also loaded straight by Node (`npm test`).
import { needsAttention, type RackRow } from './rack.ts';

/**
 * `ids` plus every ancestor of theirs, as long as the ancestor is `eligible`
 * (the same section of the list: a disabled parent is not pulled into the
 * active monitors). Stops at the first ineligible ancestor.
 */
export function withAncestors(
	targets: Target[],
	ids: ReadonlySet<TargetId>,
	eligible: ReadonlySet<TargetId>
): Set<TargetId> {
	const byId = new Map(targets.map((t) => [t.id, t]));
	const out = new Set(ids);
	for (const id of ids) {
		const seen = new Set<TargetId>([id]);
		let parent = byId.get(id)?.parent_id ?? null;
		while (parent !== null && byId.has(parent) && !seen.has(parent) && eligible.has(parent)) {
			seen.add(parent);
			out.add(parent);
			parent = byId.get(parent)?.parent_id ?? null;
		}
	}
	return out;
}

/** Flags the rows that are not among `matching`: parents brought along by `withAncestors`. */
export function markContext(rows: RackRow[], matching: ReadonlySet<TargetId>): RackRow[] {
	return rows.map((row) => (matching.has(row.target.id) ? row : { ...row, context: true }));
}

/**
 * A sorted flat list put back into its tree: every row keeps its place among
 * its siblings, children go right under their parent, one depth deeper. A
 * row whose parent is not listed stays at the top.
 */
export function nestSorted(rows: RackRow[]): RackRow[] {
	const listed = new Set(rows.map((r) => r.target.id));
	const under = new Map<TargetId | null, RackRow[]>();
	for (const row of rows) {
		const parent = row.target.parent_id ?? null;
		const key = parent !== null && listed.has(parent) ? parent : null;
		const list = under.get(key);
		if (list) list.push(row);
		else under.set(key, [row]);
	}
	const out: RackRow[] = [];
	const seen = new Set<TargetId>();
	const walk = (parent: TargetId | null, depth: number) => {
		for (const row of under.get(parent) ?? []) {
			if (seen.has(row.target.id)) continue;
			seen.add(row.target.id);
			out.push({ ...row, depth });
			walk(row.target.id, depth + 1);
		}
	};
	walk(null, 0);
	// A cycle the server would not normally allow: listed flat rather than lost.
	for (const row of rows) {
		if (seen.has(row.target.id)) continue;
		seen.add(row.target.id);
		out.push({ ...row, depth: 0 });
		walk(row.target.id, 1);
	}
	return out;
}

/**
 * The rows on screen: `rows` in tree order (children right after their
 * parent, deeper), each with its count of direct sub-monitors and of those
 * needing attention below it; the rows under a closed parent are left out.
 * `isOpen` decides for every row that has sub-monitors.
 */
export function foldRack(
	rows: RackRow[],
	isOpen: (id: TargetId, trouble: number) => boolean
): RackRow[] {
	const out: RackRow[] = [];
	// Depth of the nearest closed parent: the rows deeper than it are hidden.
	let closedAt = Infinity;
	for (let i = 0; i < rows.length; i += 1) {
		const row = rows[i];
		if (row.depth > closedAt) continue;
		closedAt = Infinity;
		let childCount = 0;
		let trouble = 0;
		for (let j = i + 1; j < rows.length && rows[j].depth > row.depth; j += 1) {
			if (rows[j].depth === row.depth + 1) childCount += 1;
			if (!rows[j].context && needsAttention(rows[j].state)) trouble += 1;
		}
		const open = childCount > 0 && isOpen(row.target.id, trouble);
		out.push({ ...row, childCount, trouble, open });
		if (childCount > 0 && !open) closedAt = row.depth;
	}
	return out;
}

const STORAGE_KEY = 'dumbmonit-targets-open';

/** Parents opened or closed by hand: id → open. */
export function loadOpenChoices(): Map<TargetId, boolean> {
	const out = new Map<TargetId, boolean>();
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		const parsed: unknown = raw ? JSON.parse(raw) : null;
		if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
			for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
				const id = Number(key);
				if (Number.isInteger(id) && typeof value === 'boolean') out.set(id, value);
			}
		}
	} catch {
		// Nothing remembered: every parent starts at its default.
	}
	return out;
}

export function saveOpenChoices(choices: ReadonlyMap<TargetId, boolean>): void {
	try {
		if (choices.size === 0) localStorage.removeItem(STORAGE_KEY);
		else localStorage.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(choices)));
	} catch {
		// Not remembered: the next visit opens every parent at its default.
	}
}
