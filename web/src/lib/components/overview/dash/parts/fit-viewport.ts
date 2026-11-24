/**
 * The height that makes a box end exactly where the window does, so the page
 * itself never scrolls (the Hybrid dashboard): the window's height, less what
 * sits above the box (header, banners, the layout switcher: its top on the
 * page) and what sits under it (the bottom padding of `<main>` and what the
 * shell puts after it, such as the version line). Fixed and hidden elements
 * take no room and are skipped.
 *
 * Measured again when the window is resized, when the page column changes
 * size (a banner appears or goes) and when the media query flips. Below it
 * (`query`), `set(null)`: the page stacks and scrolls as usual. Never under
 * `min` pixels: on a very short window the page scrolls a little rather than
 * crush the panels. Returns the cleanup.
 */
export function fitViewport(
	node: HTMLElement,
	set: (px: number | null) => void,
	{ min = 480, query = '(min-width: 64rem)' }: { min?: number; query?: string } = {}
): () => void {
	const wide = window.matchMedia(query);
	const main = node.closest('main');
	let frame = 0;

	function below(): number {
		if (!main) return 0;
		let room = Number.parseFloat(getComputedStyle(main).paddingBottom) || 0;
		for (let el = main.nextElementSibling; el; el = el.nextElementSibling) {
			const style = getComputedStyle(el);
			if (style.display === 'none' || style.position === 'fixed' || style.position === 'absolute') continue;
			room += el.getBoundingClientRect().height;
			room += (Number.parseFloat(style.marginTop) || 0) + (Number.parseFloat(style.marginBottom) || 0);
		}
		return room;
	}

	function measure() {
		cancelAnimationFrame(frame);
		frame = requestAnimationFrame(() => {
			if (!wide.matches) {
				set(null);
				return;
			}
			const top = node.getBoundingClientRect().top + window.scrollY;
			set(Math.max(min, Math.floor(window.innerHeight - top - below())));
		});
	}

	measure();
	window.addEventListener('resize', measure);
	wide.addEventListener('change', measure);
	// The box's own height does not enter the sum, so setting it settles at once.
	const observer = new ResizeObserver(measure);
	const column = main?.parentElement;
	if (column) observer.observe(column);
	return () => {
		cancelAnimationFrame(frame);
		window.removeEventListener('resize', measure);
		wide.removeEventListener('change', measure);
		observer.disconnect();
	};
}
