/**
 * Whether the status page editor differs from what is saved, to say "Unsaved
 * changes" and to ask before leaving. The order of the services lives in
 * `structure.ts`.
 */

/**
 * A comparable fingerprint of the form. Strings are trimmed the way they are
 * saved, so a trailing space does not count as a change.
 */
export function formFingerprint(fields: Record<string, unknown>): string {
	return JSON.stringify(fields, (_key, value) => (typeof value === 'string' ? value.trim() : value));
}
