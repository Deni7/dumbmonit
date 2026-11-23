/**
 * Language of a public status page. The interface already follows the visitor
 * (their stored choice, then their browser language, then English); the page's
 * own default language only matters for a visitor whose browser speaks none of
 * the interface languages. Pure, so tests load it directly.
 */

/** Where Paraglide keeps the visitor's explicit choice. */
export const LOCALE_KEY = 'PARAGLIDE_LOCALE';
/** Marks a switch made for the page default, whose stored choice must not stick. */
export const AUTO_FLAG = 'dm-status-auto-locale';

const primary = (tag: string) => tag.toLowerCase().replace('_', '-').split('-')[0];

/** True when one of the browser's languages is (or shares a primary tag with) an interface language. */
export function browserSpeaksOurs(languages: readonly string[], locales: readonly string[]): boolean {
	const ours = new Set(locales.map(primary));
	return languages.some((tag) => ours.has(primary(tag)));
}

export interface FallbackInput {
	/** The visitor's stored choice, if any. */
	stored: string | null;
	languages: readonly string[];
	locales: readonly string[];
	/** The interface language in use. */
	current: string;
	/** The page's default language (`""` or unknown reads as none). */
	defaultLang: string | undefined;
}

/** The language to switch to for the page default, or `null` to stay as is. */
export function fallbackLocale({ stored, languages, locales, current, defaultLang }: FallbackInput): string | null {
	if (stored) return null;
	if (browserSpeaksOurs(languages, locales)) return null;
	if (!defaultLang || !locales.includes(defaultLang) || defaultLang === current) return null;
	return defaultLang;
}
