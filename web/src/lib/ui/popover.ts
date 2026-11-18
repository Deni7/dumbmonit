/**
 * Places a floating panel (`position: fixed`, in the top layer when the
 * browser has popovers) under its anchor, or above it when it fits better,
 * kept 8px from the screen edges. Returns a cleanup that stops following
 * resizes and scrolls. Same rules as `Menu`.
 */
const GAP = 4;
const EDGE = 8;

export function anchorPanel(anchor: HTMLElement, panel: HTMLElement, align: 'left' | 'right' = 'left'): () => void {
	if (typeof panel.showPopover === 'function' && !panel.matches(':popover-open')) panel.showPopover();

	function place() {
		const box = anchor.getBoundingClientRect();
		const vw = document.documentElement.clientWidth;
		const vh = window.innerHeight;
		panel.style.maxHeight = '';
		const { width, height } = panel.getBoundingClientRect();
		const preferred = align === 'right' ? box.right - width : box.left;
		const left = Math.max(EDGE, Math.min(preferred, vw - width - EDGE));
		const below = vh - box.bottom - GAP - EDGE;
		const above = box.top - GAP - EDGE;
		const up = height > below && (height <= above || above > below);
		const room = Math.max(up ? above : below, 0);
		if (height > room) panel.style.maxHeight = `${room}px`;
		const top = up ? box.top - GAP - Math.min(height, room) : box.bottom + GAP;
		panel.style.left = `${Math.round(left)}px`;
		panel.style.top = `${Math.round(top)}px`;
	}

	place();
	const resize = new ResizeObserver(place);
	resize.observe(panel);
	window.addEventListener('resize', place);
	window.addEventListener('scroll', place, { capture: true, passive: true });
	return () => {
		resize.disconnect();
		window.removeEventListener('resize', place);
		window.removeEventListener('scroll', place, { capture: true });
	};
}
