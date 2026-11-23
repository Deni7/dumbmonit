/**
 * The overview dashboards' extra figures (`GET /api/overview/widgets`), shared
 * by layouts A (command), B (noc) and C (hybrid). One request per refresh,
 * polled every minute while a dashboard is shown, again when the range changes.
 * Problems, silences, SLOs, deadlines and status pages keep their own stores.
 */
import { browser } from '$app/env';
import { getOverviewWidgets, type OverviewLayout, type OverviewWidgets } from '#lib/api/index.js';
import { consoleState } from '#lib/stores/console.svelte.js';
import { startPolling } from '#lib/poll.js';

const INTERVAL_MS = 60_000;
const LAYOUT_KEY = 'dm.overview.layout';
const LAYOUTS: OverviewLayout[] = ['classic', 'command', 'noc', 'hybrid'];

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

/** The dashboard this browser shows (per viewer, kept in localStorage). */
class LayoutChoice {
	value = $state<OverviewLayout>(read());

	set(layout: OverviewLayout) {
		this.value = layout;
		try {
			localStorage.setItem(LAYOUT_KEY, layout);
		} catch {
			// Private mode or blocked storage: the choice lasts for this page only.
		}
	}
}

function read(): OverviewLayout {
	if (!browser) return 'classic';
	try {
		const stored = localStorage.getItem(LAYOUT_KEY) as OverviewLayout | null;
		return stored && LAYOUTS.includes(stored) ? stored : 'classic';
	} catch {
		return 'classic';
	}
}

export const layoutChoice = new LayoutChoice();
export { LAYOUTS };
