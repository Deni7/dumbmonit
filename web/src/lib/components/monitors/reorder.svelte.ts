/**
 * Manual order of the monitors list, by pointer and by keyboard.
 *
 * Drag-and-drop is pointer-based (mouse and touch — HTML5 drag-and-drop does
 * not work on touch). The row itself is the drag surface: a mouse or a pen
 * arms a drag once the pointer has moved past a small threshold, so a plain
 * click still opens the device; a touch needs a short hold first, so a tap
 * (or a scroll starting on the row) still works. Row and folder-header
 * elements are reported by the components themselves and hit-tested on every
 * pointer move; rows never reflow mid-drag, so their rects stay valid for the
 * whole gesture.
 *
 * Keyboard: Space picks a row up, arrows move it within its scope (persisted
 * immediately, same as a drag), Space/Enter drops it, Escape restores the
 * scope's original order. No separate handle: the row itself is the one
 * focus stop, with an aria-live announcement (`announce`).
 */
import { m } from '#lib/paraglide/messages.js';
import type { Target, TargetId } from '#lib/api/index.js';
import type { TargetState } from '#lib/format.js';
import type { ReorderControls } from '#lib/components/devices/RackList.svelte';
import { nearestRow } from '#lib/components/devices/rack.js';
import { folderKeyOf, moveWithinScope, reorderByDrop, reorderScope } from '#lib/components/devices/folders.js';

export type DropSlot = { kind: 'row'; id: TargetId; before: boolean } | { kind: 'folder'; key: string };

export interface ReorderDeps {
	/** Reordering is offered right now (administrator, compact layout, unfiltered, unsorted). */
	enabled: () => boolean;
	/** The tree on screen: it decides the siblings (a hosted database is a child there). */
	shown: () => Target[];
	/** The monitors as saved, optimistic overrides applied: what a move sends back. */
	saved: () => Target[];
	stateOf: (target: Target) => TargetState;
	persistOrder: (order: TargetId[]) => Promise<void>;
	moveToFolder: (target: Target, key: string) => Promise<void>;
}

export class ListReorder {
	#deps: ReorderDeps;

	dragging = $state<TargetId | null>(null);
	dragLabel = $state('');
	pointerPos = $state<{ x: number; y: number } | null>(null);
	dropSlot = $state<DropSlot | null>(null);
	/** Device picked up with the keyboard, waiting for arrow keys. */
	pickedUp = $state<TargetId | null>(null);
	/** Read out by the page's aria-live line. */
	announce = $state('');

	readonly rowDropSlot = $derived(this.dropSlot?.kind === 'row' ? this.dropSlot : null);

	/** Devices needing attention float to the top regardless of a manual drop: shown as a hint on the ghost. */
	readonly dropBlockedHint = $derived.by(() => {
		const slot = this.dropSlot;
		const dragging = this.dragging;
		if (dragging === null || !slot || slot.kind !== 'row' || slot.id === dragging) return false;
		const shown = this.#deps.shown();
		const dragged = shown.find((t) => t.id === dragging);
		if (!dragged) return false;
		if (!reorderScope(shown, dragged).some((t) => t.id === slot.id)) return false;
		return reorderByDrop(shown, this.#deps.stateOf, dragging, slot.id) === null;
	});

	#pickedSnapshot: TargetId[] | null = null;
	#rowEls = new Map<TargetId, HTMLElement>();
	#headerEls = new Map<string, HTMLElement>();
	#longPressTimer: ReturnType<typeof setTimeout> | null = null;
	// Set the moment a pointer drag actually starts (threshold crossed, or the
	// touch long-press fired), so the click that follows the pointerup — if
	// the browser still fires one over the same row — can be swallowed
	// instead of opening the device.
	#justDragged = false;
	#autoScrollRaf: number | null = null;

	constructor(deps: ReorderDeps) {
		this.#deps = deps;
	}

	registerRow = (id: TargetId, el: HTMLElement | null) => {
		if (el) this.#rowEls.set(id, el);
		else this.#rowEls.delete(id);
	};

	registerHeader = (key: string, el: HTMLElement | null) => {
		if (el) this.#headerEls.set(key, el);
		else this.#headerEls.delete(key);
	};

	#computeDropSlot(x: number, y: number): DropSlot | null {
		for (const [key, el] of this.#headerEls) {
			const r = el.getBoundingClientRect();
			if (y >= r.top && y <= r.bottom) return { kind: 'folder', key };
		}
		const rects = [...this.#rowEls.entries()].map(([id, el]) => {
			const r = el.getBoundingClientRect();
			return { id, top: r.top, height: r.height };
		});
		const slot = nearestRow(rects, y);
		return slot ? { kind: 'row', ...slot } : null;
	}

	onRowPointerDown = (id: TargetId, event: PointerEvent) => {
		if (!this.#deps.enabled()) return;
		if (event.button !== 0) return;
		const startX = event.clientX;
		const startY = event.clientY;
		const touch = event.pointerType === 'touch';
		const threshold = touch ? 10 : 6;
		let settled = false;

		const onmove = (e: PointerEvent) => {
			if (settled) return;
			if (Math.hypot(e.clientX - startX, e.clientY - startY) <= threshold) return;
			if (touch) {
				// Moved too soon: this is a scroll, not a hold. Disarm.
				settle();
			} else {
				settle();
				this.#beginDrag(id, e);
			}
		};
		const onup = () => settle();
		const settle = () => {
			settled = true;
			if (this.#longPressTimer) clearTimeout(this.#longPressTimer);
			this.#longPressTimer = null;
			window.removeEventListener('pointermove', onmove);
			window.removeEventListener('pointerup', onup);
		};
		window.addEventListener('pointermove', onmove);
		window.addEventListener('pointerup', onup, { once: true });

		if (touch) {
			this.#longPressTimer = setTimeout(() => {
				if (settled) return;
				settle();
				this.#beginDrag(id, event);
			}, 350);
		}
	};

	#beginDrag(id: TargetId, event: PointerEvent) {
		event.preventDefault();
		const target = this.#deps.saved().find((t) => t.id === id);
		if (!target) return;
		this.#justDragged = true;
		this.dragging = id;
		this.dragLabel = target.name;
		this.pointerPos = { x: event.clientX, y: event.clientY };
		this.dropSlot = this.#computeDropSlot(event.clientX, event.clientY);
	}

	/** Swallows the click a pointer drag leaves in its wake (see `#justDragged`). */
	onRowClick = (_id: TargetId, event: MouseEvent) => {
		if (!this.#justDragged) return;
		this.#justDragged = false;
		event.preventDefault();
		event.stopPropagation();
	};

	#onPointerMove = (e: PointerEvent) => {
		this.pointerPos = { x: e.clientX, y: e.clientY };
		this.dropSlot = this.#computeDropSlot(e.clientX, e.clientY);
	};

	#cancelDrag = () => {
		this.dragging = null;
		this.pointerPos = null;
		this.dropSlot = null;
		// Safety net: a click should follow the pointerup within the same
		// task, if the browser fires one at all — drop the flag afterwards so
		// it never lingers onto some later, unrelated click.
		setTimeout(() => (this.#justDragged = false), 0);
	};

	#finishDrag = () => {
		const draggedId = this.dragging;
		const slot = this.dropSlot;
		this.#cancelDrag();
		if (draggedId !== null && slot) this.#applyDrop(draggedId, slot);
	};

	#applyDrop(draggedId: TargetId, slot: DropSlot) {
		// The tree on screen decides the siblings (a hosted database is a child
		// there); the saved monitor is the one sent back.
		const shown = this.#deps.shown();
		const dragged = shown.find((t) => t.id === draggedId);
		const saved = this.#deps.saved().find((t) => t.id === draggedId);
		if (!dragged || !saved) return;
		if (slot.kind === 'row') {
			if (slot.id === draggedId) return;
			const order = reorderByDrop(shown, this.#deps.stateOf, draggedId, slot.id);
			if (order) {
				void this.#deps.persistOrder(order);
				return;
			}
			// Not reorderable in place (a different folder, parent or state tier):
			// fall back to a folder-level move, root devices only — a child's
			// folder is its parent's, dragging it onto another one is a no-op.
			if (dragged.parent_id !== null) return;
			const over = shown.find((t) => t.id === slot.id);
			if (!over) return;
			const key = folderKeyOf(shown, over);
			if (key !== (saved.group_name || '')) void this.#deps.moveToFolder(saved, key);
			return;
		}
		if (dragged.parent_id !== null) return;
		if ((saved.group_name || '') !== slot.key) void this.#deps.moveToFolder(saved, slot.key);
	}

	#startAutoScroll() {
		const EDGE = 72;
		const MAX_SPEED = 16;
		const tick = () => {
			if (this.dragging === null) {
				this.#autoScrollRaf = null;
				return;
			}
			if (this.pointerPos) {
				const { y } = this.pointerPos;
				const h = window.innerHeight;
				if (y < EDGE) window.scrollBy(0, -MAX_SPEED * (1 - y / EDGE));
				else if (y > h - EDGE) window.scrollBy(0, MAX_SPEED * (1 - (h - y) / EDGE));
			}
			this.#autoScrollRaf = requestAnimationFrame(tick);
		};
		this.#autoScrollRaf = requestAnimationFrame(tick);
	}

	#stopAutoScroll() {
		if (this.#autoScrollRaf !== null) cancelAnimationFrame(this.#autoScrollRaf);
		this.#autoScrollRaf = null;
	}

	/**
	 * While a drag runs, follows the pointer over the whole window and
	 * scrolls near the edges. Call from an `$effect`: it reads `dragging`.
	 */
	listen(): (() => void) | undefined {
		if (this.dragging === null) return;
		const onpointerup = () => this.#finishDrag();
		const onpointercancel = () => this.#cancelDrag();
		const onkeydown = (e: KeyboardEvent) => {
			if (e.key === 'Escape') this.#cancelDrag();
		};
		window.addEventListener('pointermove', this.#onPointerMove);
		window.addEventListener('pointerup', onpointerup);
		window.addEventListener('pointercancel', onpointercancel);
		window.addEventListener('keydown', onkeydown);
		this.#startAutoScroll();
		return () => {
			window.removeEventListener('pointermove', this.#onPointerMove);
			window.removeEventListener('pointerup', onpointerup);
			window.removeEventListener('pointercancel', onpointercancel);
			window.removeEventListener('keydown', onkeydown);
			this.#stopAutoScroll();
		};
	}

	onRowKeyDown = (id: TargetId, event: KeyboardEvent) => {
		if (!this.#deps.enabled()) return;
		// As on screen: a hosted database moves among its machine's sub-monitors.
		const shown = this.#deps.shown();
		const target = shown.find((t) => t.id === id);
		if (!target) return;
		if (event.key === ' ' || event.key === 'Enter') {
			event.preventDefault();
			if (this.pickedUp === id) {
				this.pickedUp = null;
				this.#pickedSnapshot = null;
				this.announce = m.devices_targets_announce_dropped({ name: target.name });
			} else {
				this.pickedUp = id;
				this.#pickedSnapshot = reorderScope(shown, target).map((t) => t.id);
				this.announce = m.devices_targets_announce_picked({ name: target.name });
			}
			return;
		}
		if (this.pickedUp !== id) return;
		if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
			event.preventDefault();
			const delta = event.key === 'ArrowUp' ? -1 : 1;
			const order = moveWithinScope(shown, this.#deps.stateOf, id, delta);
			if (order) {
				void this.#deps.persistOrder(order);
				this.announce = m.devices_targets_announce_moved({ name: target.name, position: order.indexOf(id) + 1, total: order.length });
			} else {
				this.announce = m.devices_targets_announce_blocked({ name: target.name });
			}
			return;
		}
		if (event.key === 'Escape') {
			event.preventDefault();
			this.pickedUp = null;
			if (this.#pickedSnapshot) void this.#deps.persistOrder(this.#pickedSnapshot);
			this.#pickedSnapshot = null;
			this.announce = m.devices_targets_announce_cancelled({ name: target.name });
		}
	};

	/** What each row of the rack needs to take part. */
	controls(): ReorderControls {
		return {
			dragging: this.dragging,
			pickedUp: this.pickedUp,
			dropSlot: this.rowDropSlot,
			onRowPointerDown: this.onRowPointerDown,
			onRowKeyDown: this.onRowKeyDown,
			onRowClick: this.onRowClick,
			registerRow: this.registerRow
		};
	}
}
