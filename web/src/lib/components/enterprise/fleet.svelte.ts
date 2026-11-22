/**
 * The console's fleet figures, shared by the overview and the context panel
 * so both read the same numbers from one set of queries. Refreshed every
 * minute while LIVE is on, and again as soon as the range changes.
 */
import { browser } from '$app/env';
import { listAlertHistory, type AlertHistoryEntry, type TargetId } from '#lib/api/index.js';
import { consoleState } from '#lib/stores/console.svelte.js';
import { startPolling } from '#lib/poll.js';
import { loadFigures, loadLatency, type FleetFigures } from './fleet.js';

const INTERVAL_MS = 60_000;

class FleetStore {
	latency = $state<Map<TargetId, number[]>>(new Map());
	figures = $state<FleetFigures>({ p95: new Map(), uptime30: new Map(), fleetP95: null });
	history = $state<AlertHistoryEntry[]>([]);
	loading = $state(true);
	error = $state<unknown>(null);
	#subscribers = 0;
	#controller: AbortController | null = null;
	#loadedRange = '';
	/** Range of the refresh in flight, so a second view starting at once does not repeat it. */
	#loadingRange = '';
	#stopPolling: (() => void) | undefined;

	/** Resolves to `false` when the figures could not be read. */
	async refresh(): Promise<boolean> {
		this.#controller?.abort();
		const controller = new AbortController();
		this.#controller = controller;
		this.#loadingRange = consoleState.range;
		const seconds = consoleState.rangeSeconds;
		const since = new Date(Date.now() - Math.max(seconds, 60 * 86400) * 1000).toISOString();
		try {
			const [latency, figures, history] = await Promise.all([
				loadLatency(seconds, controller.signal),
				loadFigures(seconds, controller.signal),
				listAlertHistory({ since, limit: 1000 }, controller.signal).catch(() => [] as AlertHistoryEntry[])
			]);
			this.latency = latency;
			this.figures = figures;
			this.history = history;
			this.error = null;
			this.#loadedRange = consoleState.range;
			return true;
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return true;
			this.error = cause;
			return false;
		} finally {
			if (this.#controller === controller) {
				this.loading = false;
				this.#loadingRange = '';
			}
		}
	}

	/** Whether the figures of the current range are neither loaded nor being loaded. */
	#stale(): boolean {
		return this.#loadedRange !== consoleState.range && this.#loadingRange !== consoleState.range;
	}

	/** Starts refreshing while at least one view needs the figures; returns the stop function. */
	start(): () => void {
		if (!browser) return () => {};
		this.#subscribers += 1;
		if (this.#subscribers === 1) {
			// The first load always runs; the next ones only while LIVE is on.
			let first = true;
			this.#stopPolling = startPolling(() => {
				if (!first && !consoleState.live) return true;
				first = false;
				return this.refresh();
			}, INTERVAL_MS);
		} else if (this.#stale()) {
			void this.refresh();
		}
		return () => {
			this.#subscribers -= 1;
			if (this.#subscribers > 0) return;
			this.#stopPolling?.();
			this.#stopPolling = undefined;
			this.#controller?.abort();
		};
	}

	/** Called when the range changes. */
	rangeChanged() {
		if (this.#subscribers > 0 && this.#stale()) {
			this.loading = true;
			void this.refresh();
		}
	}
}

export const fleetStore = new FleetStore();
