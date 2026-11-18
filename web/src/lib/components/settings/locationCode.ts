/**
 * Two-letter badge code of a location. Mirrors `default_code` in
 * `crates/server/src/api/locations.rs`: the first two letters or digits of
 * the name, accents removed, upper case ("Crissier" gives "CR").
 */
export function defaultLocationCode(name: string): string {
	return name
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.replace(/[^A-Za-z0-9]/g, '')
		.slice(0, 2)
		.toUpperCase();
}

/** What a typed code becomes: at most two letters or digits, upper case. */
export function normalizeLocationCode(raw: string): string {
	return defaultLocationCode(raw);
}
