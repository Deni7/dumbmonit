/**
 * Keys for the keyed `{#each}` blocks of the overview, unique within their
 * list whatever the data says. Svelte stops the page (`each_key_duplicate`)
 * on two equal keys, and the natural ids of the overview's rows can repeat:
 * a certificate probed from two places (this site and a relay) gives two
 * deadlines with the same kind, monitor and name; two relays can report the
 * same site under the same key; an alert and a monitor can share a target id.
 *
 * The first row with a key keeps it; the next ones get `key#2`, `key#3`…
 * (skipping any that the list already uses), so a row keeps its identity
 * across refreshes as long as its rank among its equals does not change.
 * Pure on purpose, so it runs under `node --test`.
 */
export function uniqueKeys<T>(items: readonly T[], keyOf: (item: T, index: number) => string | number): string[] {
	const taken = new Set<string>();
	const natural = items.map((item, index) => String(keyOf(item, index)));
	for (const key of natural) taken.add(key);
	const seen = new Set<string>();
	const out: string[] = [];
	for (const key of natural) {
		if (!seen.has(key)) {
			seen.add(key);
			out.push(key);
			continue;
		}
		let n = 2;
		let candidate = `${key}#${n}`;
		while (taken.has(candidate)) {
			n += 1;
			candidate = `${key}#${n}`;
		}
		taken.add(candidate);
		seen.add(candidate);
		out.push(candidate);
	}
	return out;
}
