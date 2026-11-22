/**
 * Single-key shortcuts that act on the current page, not on navigation:
 * `/` focuses the page's search field, `j` / `k` move between list rows and
 * `a` acknowledges the focused alert row. Pages opt in with data attributes:
 *
 * - `data-page-search` on the main search input,
 * - `data-list-row` on each row (a `<li>`),
 * - `data-ack` on the row's acknowledge button.
 *
 * The caller (the root layout) has already ruled out text fields, dialogs and
 * chorded keys. Returns true when the key did something.
 */

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), [tabindex="0"]';

function visible(el: Element): boolean {
	return el instanceof HTMLElement && el.offsetParent !== null;
}

function rows(): HTMLElement[] {
	return Array.from(document.querySelectorAll<HTMLElement>('[data-list-row]')).filter(visible);
}

function focusTarget(row: HTMLElement): HTMLElement | null {
	return row.querySelector<HTMLElement>('[data-row-focus]') ?? row.querySelector<HTMLElement>(FOCUSABLE);
}

function currentRow(): HTMLElement | null {
	const active = document.activeElement;
	return active instanceof HTMLElement ? active.closest<HTMLElement>('[data-list-row]') : null;
}

export function focusPageSearch(): boolean {
	const field = Array.from(document.querySelectorAll<HTMLElement>('[data-page-search]')).find(visible);
	if (!field) return false;
	field.focus();
	if (field instanceof HTMLInputElement) field.select();
	return true;
}

export function moveRowFocus(delta: 1 | -1): boolean {
	const list = rows();
	if (list.length === 0) return false;
	const here = currentRow();
	const index = here ? list.indexOf(here) : -1;
	const next = list[Math.min(list.length - 1, Math.max(0, index === -1 ? (delta === 1 ? 0 : list.length - 1) : index + delta))];
	const target = focusTarget(next);
	if (!target) return false;
	target.focus();
	next.scrollIntoView({ block: 'nearest' });
	return true;
}

export function ackFocusedRow(): boolean {
	const row = currentRow();
	const button = row?.querySelector<HTMLButtonElement>('[data-ack]:not([aria-pressed="true"])');
	if (!button) return false;
	button.click();
	return true;
}

/** Handles one keydown; true when it was consumed. */
export function handlePageKey(event: KeyboardEvent): boolean {
	if (event.ctrlKey || event.metaKey || event.altKey) return false;
	switch (event.key) {
		case '/':
			return focusPageSearch();
		case 'j':
			return moveRowFocus(1);
		case 'k':
			return moveRowFocus(-1);
		case 'a':
			return ackFocusedRow();
		default:
			return false;
	}
}
