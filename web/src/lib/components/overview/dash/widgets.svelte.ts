/**
 * The overview's extra figures (`GET /api/overview/widgets`): sites, services,
 * events, availability and the alerts opened per day. One request per
 * refresh, polled every minute while the overview is shown, again when the
 * range changes. Problems, silences and deadlines keep their own stores.
 */
import { browser } from '$app/env';
import { getOverviewWidgets, type OverviewWidgets } from '#lib/api/index.js';
import { consoleState } from '#lib/stores/console.svelte.js';
import { startPolling } from '#lib/poll.js';

const INTERVAL_MS = 60_000;

class WidgetsStore {
	data = $state<OverviewWidgets | null>(null);
	loading = $state(true);
	error = $state<unknown>(null);
	#subscribers = 0;
	#controller: AbortController | null = null;
	#stopPolling: (() => void) | undefined;

	async refresh(): Promise<boolean> {
		this.#controller?.abort();
		const controller = new AbortController();
		this.#controller = controller;
		try {
			const data = await getOverviewWidgets(consoleState.range, controller.signal);
			if (controller.signal.aborted) return false;
			this.data = data;
			this.error = null;
			return true;
		} catch (error) {
			if (controller.signal.aborted) return false;
			this.error = error;
			return false;
		} finally {
			if (this.#controller === controller) this.loading = false;
		}
	}

	/** Call from a component `$effect`; returns the stop function. */
	start(): () => void {
		this.#subscribers += 1;
		if (this.#subscribers === 1 && browser) {
			this.#stopPolling = startPolling(() => this.refresh(), INTERVAL_MS);
		}
		return () => {
			this.#subscribers -= 1;
			if (this.#subscribers === 0) {
				this.#stopPolling?.();
				this.#stopPolling = undefined;
				this.#controller?.abort();
			}
		};
	}
}

export const widgetsStore = new WidgetsStore();

