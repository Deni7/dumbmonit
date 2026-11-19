/**
 * The instance's accent colour (Settings › Appearance), pure so it is tested
 * with `node --test`. One chosen hue is turned into what the console reads:
 * the fill of the solid button and the focus ring, and the ink laid over that
 * fill, once for the light console and once for the dark one.
 *
 * Rules, whatever the hue picked:
 * - the fill keeps ≥ 3:1 against the console's surface in its light (a pale
 *   yellow is darkened on light, a deep navy is lightened on dark), by mixing
 *   toward black or white so the hue stays;
 * - the ink on the fill is the near-black or the near-white that reads
 *   ≥ 4.5:1 on it (one of the two always does).
 * - the accent as text (active navigation item, links, tab labels) keeps
 *   ≥ 5:1 against the raised surface in its light.
 * Status colours (red, amber, green) are never touched.
 *
 * Applied by `applyAccent` as CSS variables on `<html>` (app.css reads them
 * under `html[data-accent]`); the derived values are cached per browser so the
 * bootstrap script in app.html can apply them before the first paint: keep the
 * key and the variable names in step with it.
 */

/** Per browser; the derived values as JSON, absent for the default look. */
export const ACCENT_KEY = 'dumbmonit-accent';

/** Surfaces the fill must stand out from, in each light (app.css `--c-surface`). */
const SURFACE_LIGHT = '#ffffff';
const SURFACE_DARK = '#0b0d10';
/** The tinted rows text sits on: the console's `--c-surface-2` in each light. */
const RAISED_LIGHT = '#efeff1';
const RAISED_DARK = '#111419';
const INK_DARK = '#07080a';
const INK_LIGHT = '#ffffff';

/** The preset hues. `''` is the default look: the console's own ink, no tint. */
export const ACCENT_PALETTE: readonly { id: string; hex: string }[] = [
	{ id: 'default', hex: '' },
	{ id: 'blue', hex: '#2563eb' },
	{ id: 'sky', hex: '#0284c7' },
	{ id: 'teal', hex: '#0d9488' },
	{ id: 'indigo', hex: '#5b5bd6' },
	{ id: 'violet', hex: '#8b4fd9' },
	{ id: 'pink', hex: '#d6409f' },
	{ id: 'orange', hex: '#d9640a' },
	{ id: 'slate', hex: '#64748b' }
];

export interface DerivedAccent {
	/** Fill and focus ring on the light console. */
	lightFill: string;
	/** Ink over that fill. */
	lightOn: string;
	darkFill: string;
	darkOn: string;
	/** The accent as text on the light and dark console. */
	lightInk: string;
	darkInk: string;
}

/** `#rrggbb` in lower case, or `''` for anything else (including the empty default). */
export function parseAccent(raw: string | null | undefined): string {
	const value = (raw ?? '').trim().toLowerCase();
	return /^#[0-9a-f]{6}$/.test(value) ? value : '';
}

type Rgb = [number, number, number];

function toRgb(hex: string): Rgb {
	const n = parseInt(hex.slice(1), 16);
	return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function toHex([r, g, b]: Rgb): string {
	return '#' + [r, g, b].map((c) => Math.round(Math.min(255, Math.max(0, c))).toString(16).padStart(2, '0')).join('');
}

/** WCAG relative luminance. */
export function luminance(hex: string): number {
	const [r, g, b] = toRgb(hex).map((c) => {
		const s = c / 255;
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two `#rrggbb` colours, 1 to 21. */
export function contrast(a: string, b: string): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

function mix(hex: string, target: Rgb, amount: number): string {
	const from = toRgb(hex);
	return toHex(from.map((c, i) => c + (target[i] - c) * amount) as Rgb);
}

/** Moves `hex` toward black or white, a little at a time, until it holds `min`:1 against `against`. */
function holdContrast(hex: string, against: string, min: number, toward: Rgb): string {
	let out = hex;
	for (let step = 1; step <= 20 && contrast(out, against) < min; step++) out = mix(hex, toward, step / 20);
	return out;
}

/** The ink for text laid on `fill`: whichever of the near-black and the white reads better. */
export function inkOn(fill: string): string {
	return contrast(fill, INK_DARK) >= contrast(fill, INK_LIGHT) ? INK_DARK : INK_LIGHT;
}

/** What each console reads for a chosen hue. `null` for the default look. */
export function deriveAccent(raw: string): DerivedAccent | null {
	const hex = parseAccent(raw);
	if (!hex) return null;
	// A mid-tone can hold 3:1 against the page and still read poorly under either ink:
	// it then darkens until white text holds 4.5:1 (on dark, it lightens for the near-black).
	const legible = (fill: string, toward: Rgb, ink: string) =>
		contrast(fill, inkOn(fill)) >= 4.5 ? fill : holdContrast(fill, ink, 4.5, toward);
	const lightFill = legible(holdContrast(hex, SURFACE_LIGHT, 3, [0, 0, 0]), [0, 0, 0], INK_LIGHT);
	const darkFill = legible(holdContrast(hex, SURFACE_DARK, 3, [255, 255, 255]), [255, 255, 255], INK_DARK);
	return {
		lightFill,
		lightOn: inkOn(lightFill),
		darkFill,
		darkOn: inkOn(darkFill),
		lightInk: holdContrast(hex, RAISED_LIGHT, 5, [0, 0, 0]),
		darkInk: holdContrast(hex, RAISED_DARK, 5, [255, 255, 255])
	};
}

/** Puts the accent on `<html>` and remembers it for the next first paint. `''` restores the default look. */
export function applyAccent(raw: string): void {
	if (typeof document === 'undefined') return;
	const root = document.documentElement;
	const derived = deriveAccent(raw);
	const names = ['--accent-light', '--accent-light-on', '--accent-dark', '--accent-dark-on', '--accent-light-ink', '--accent-dark-ink'];
	if (!derived) {
		root.removeAttribute('data-accent');
		for (const name of names) root.style.removeProperty(name);
	} else {
		root.setAttribute('data-accent', '');
		root.style.setProperty(names[0], derived.lightFill);
		root.style.setProperty(names[1], derived.lightOn);
		root.style.setProperty(names[2], derived.darkFill);
		root.style.setProperty(names[3], derived.darkOn);
		root.style.setProperty(names[4], derived.lightInk);
		root.style.setProperty(names[5], derived.darkInk);
	}
	try {
		if (derived) localStorage.setItem(ACCENT_KEY, JSON.stringify(derived));
		else localStorage.removeItem(ACCENT_KEY);
	} catch {
		// Storage unavailable: the accent simply appears after the settings are read.
	}
}
