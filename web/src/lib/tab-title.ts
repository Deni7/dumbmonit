/**
 * Browser tab title and favicon: the instance name in page titles, the count
 * of open critical alerts in front of it, a red dot on the icon. Pure (no
 * DOM, no store) so `node --test` loads it directly.
 */

/** Name used when the instance has none. */
export const BRAND = 'DumbMonit';

/** `Page · Instance name`: every page title goes through here. */
export function pageTitle(page: string, instanceName?: string | null): string {
	const name = instanceName?.trim() || BRAND;
	return page ? `${page} · ${name}` : name;
}

const BADGE = /^\(\d+\+?\)\s/;

/** The same title with `(n) ` in front when `count` is above zero (99+ capped); never twice. */
export function withCount(title: string, count: number): string {
	const base = title.replace(BADGE, '');
	if (count <= 0) return base;
	return `(${count > 99 ? '99+' : count}) ${base}`;
}

/** Same artwork as static/favicon.svg. */
const FAVICON =
	'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">' +
	'<rect width="64" height="64" rx="16" fill="#6f83a3"/>' +
	'<circle cx="24" cy="30" r="14" fill="#fff" stroke="#1e2640" stroke-width="4"/>' +
	'<circle cx="45" cy="30" r="11" fill="#fff" stroke="#1e2640" stroke-width="4"/>' +
	'<circle cx="20" cy="27" r="5" fill="#1e2640"/>' +
	'<circle cx="48" cy="27" r="4" fill="#1e2640"/>' +
	'<path d="M27 40c3-3 11-3 14 0l-7 12z" fill="#e97b3a" stroke="#1e2640" stroke-width="3" stroke-linejoin="round"/>';

/** The favicon with a red dot in the corner, as a `data:` URL. */
export function dottedFaviconHref(): string {
	const svg = `${FAVICON}<circle cx="50" cy="14" r="12" fill="#e5484d" stroke="#fff" stroke-width="4"/></svg>`;
	return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
