import { instanceStore } from '#lib/stores/instance.svelte.js';
import { pageTitle } from '#lib/tab-title.js';

/**
 * Title of a page with the instance's name, for `<svelte:head>`:
 * `<title>{docTitle(m.alerts_page_title())}</title>`. Reactive: it follows the
 * name once the settings are read.
 */
export function docTitle(page: string): string {
	return pageTitle(page, instanceStore.name);
}
