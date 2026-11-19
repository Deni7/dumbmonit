/** The console's six states, from a monitor, its last probe and its firing alerts. */
import type { Target } from '#lib/api/index.js';
import type { TargetAlertCounts } from '#lib/alerts-attention.js';
import { displayState, type ProbeStatus } from '#lib/format.js';

export type ConsoleStatus = 'down' | 'degraded' | 'config' | 'waiting' | 'up' | 'paused';

export function consoleStatus(target: Target, probe: ProbeStatus | undefined, alerts?: TargetAlertCounts): ConsoleStatus {
	const state = displayState(target, probe);
	switch (state) {
		case 'down':
		case 'offline':
			return 'down';
		case 'misconfigured':
			return 'config';
		case 'disabled':
			return 'paused';
		case 'pending':
		case 'unknown':
			return 'waiting';
		case 'online':
			return alerts && alerts.firing > 0 ? 'degraded' : 'up';
	}
}
