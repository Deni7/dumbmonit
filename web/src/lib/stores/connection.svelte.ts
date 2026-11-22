import { browser } from '$app/env';
import { onConnectionChange } from '#lib/api/connection.js';

/**
 * "Connection lost" state of the console: the browser reports itself offline,
 * or the request layer saw several network failures in a row. Reactive.
 */
class ConnectionStore {
	offline = $state(false);
	failing = $state(false);

	get lost(): boolean {
		return this.offline || this.failing;
	}

	/** Starts listening; returns the stop function for an `$effect`. */
	watch(): () => void {
		if (!browser) return () => {};
		this.offline = !navigator.onLine;
		const down = () => (this.offline = true);
		const up = () => (this.offline = false);
		window.addEventListener('offline', down);
		window.addEventListener('online', up);
		const stop = onConnectionChange((lost) => (this.failing = lost));
		return () => {
			window.removeEventListener('offline', down);
			window.removeEventListener('online', up);
			stop();
		};
	}
}

export const connection = new ConnectionStore();
