/**
 * One polling loop for every panel that refreshes itself: runs `load`
 * at once and then every `ms`, never two loads at the same time, pauses
 * while the tab is hidden (and refreshes once on return if the last
 * refresh is older than `ms`), backs off while loads keep failing, and
 * aborts the request in flight when stopped.
 *
 *   $effect(() => {
 *     void target.id;
 *     return startPolling(load, 60_000);
 *   });
 *
 * `load` receives the AbortSignal. It reports a failure by rejecting or by
 * returning `false`; any other result (`undefined` included) is a success.
 * A panel that catches its own errors to show them must therefore still say
 * so, or the backoff never starts:
 *
 *   async function load(signal?: AbortSignal) {
 *     try {
 *       view = await getView(target.id, signal);
 *       error = null;
 *     } catch (cause) {
 *       if (signal?.aborted) return;
 *       error = cause;
 *       return false;
 *     }
 *   }
 *
 * `{ immediate: false }` waits one interval before the first load, for a
 * panel whose data was just read by something else.
 */

/** Longest wait between two loads while they keep failing. */
export const MAX_BACKOFF_MS = 300_000;

/** Wait before the next load: `ms`, doubled for every failure in a row, capped at 5 min (never below `ms`). */
export function pollDelay(ms: number, failures: number): number {
	if (failures <= 0) return ms;
	const cap = Math.max(ms, MAX_BACKOFF_MS);
	return Math.min(ms * 2 ** Math.min(failures, 30), cap);
}

/** Time left before a visible tab owes a refresh (0 = now). */
export function pollRemaining(delay: number, lastEnd: number, now: number): number {
	return Math.max(0, delay - (now - lastEnd));
}

export type PollLoad = (signal: AbortSignal) => unknown;

export interface PollOptions {
	/** Load at once (the default), or only after a first interval. */
	immediate?: boolean;
}

/** Starts polling; returns the cleanup to hand back from `$effect`. */
export function startPolling(load: PollLoad, ms: number, options: PollOptions = {}): () => void {
	const controller = new AbortController();
	const hasDocument = typeof document !== 'undefined';
	let timer: ReturnType<typeof setTimeout> | undefined;
	let running = false;
	let stopped = false;
	let failures = 0;
	let lastEnd = 0;

	function schedule(wait: number) {
		clearTimeout(timer);
		timer = undefined;
		if (stopped || (hasDocument && document.hidden)) return;
		timer = setTimeout(run, wait);
	}

	async function run() {
		timer = undefined;
		if (stopped || running) return;
		running = true;
		let ok = true;
		try {
			ok = (await load(controller.signal)) !== false;
		} catch {
			ok = !!controller.signal.aborted;
		}
		running = false;
		if (stopped) return;
		failures = ok ? 0 : failures + 1;
		lastEnd = Date.now();
		schedule(pollDelay(ms, failures));
	}

	function onVisibility() {
		if (stopped) return;
		if (document.hidden) {
			clearTimeout(timer);
			timer = undefined;
		} else if (!running) {
			schedule(pollRemaining(pollDelay(ms, failures), lastEnd, Date.now()));
		}
	}

	if (hasDocument) document.addEventListener('visibilitychange', onVisibility);
	if (options.immediate === false) {
		lastEnd = Date.now();
		schedule(ms);
	} else {
		void run();
	}

	return () => {
		stopped = true;
		clearTimeout(timer);
		if (hasDocument) document.removeEventListener('visibilitychange', onVisibility);
		controller.abort();
	};
}
