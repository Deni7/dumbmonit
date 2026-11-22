/**
 * Whether the server can be reached, as the request layer sees it: a run of
 * consecutive network failures (status 0, not an HTTP error) means the link
 * is down; the next success resets it. Pure, no DOM, so `node --test` loads it.
 */

/** Consecutive failures that count as "connection lost". */
export const FAILURE_THRESHOLD = 3;

export function createConnectionTracker(
	threshold = FAILURE_THRESHOLD,
	onChange: (lost: boolean) => void = () => {}
) {
	let failures = 0;
	let lost = false;
	const set = (next: boolean) => {
		if (next === lost) return;
		lost = next;
		onChange(lost);
	};
	return {
		failure() {
			failures += 1;
			if (failures >= threshold) set(true);
		},
		success() {
			failures = 0;
			set(false);
		},
		get lost() {
			return lost;
		}
	};
}

type Listener = (lost: boolean) => void;
const listeners = new Set<Listener>();

const tracker = createConnectionTracker(FAILURE_THRESHOLD, (lost) => {
	for (const listener of listeners) listener(lost);
});

/** Called by `request` when the network itself failed (never for an aborted request). */
export function reportNetworkFailure(): void {
	tracker.failure();
}

/** Called by `request` when the server answered, whatever the status. */
export function reportNetworkSuccess(): void {
	tracker.success();
}

/** Subscribes to changes; returns the unsubscribe function. */
export function onConnectionChange(listener: Listener): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}
