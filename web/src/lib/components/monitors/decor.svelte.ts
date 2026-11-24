/**
 * What decorates the monitor list on top of the shared store: sparklines (tiles), the 24 hourly
 * bars (table), security badges, kind labels and option keys. Every read is a decoration: a
 * failure never takes the list down.
 */
import { untrack } from 'svelte';
import { startPolling } from '#lib/poll.js';
import { kindLabel } from '#lib/kind-label.js';
import { listCollectors, type SecurityGrade, type Target, type TargetId } from '#lib/api/index.js';
import { loadSparklines } from '#lib/metrics.js';
import { loadRiver, DAY_SECONDS, type RiverCell } from '#lib/components/enterprise/fleet.js';
import { getSecuritySummary } from '#lib/api/security.js';
import type { Serie } from '#lib/components/Chart.svelte';
import type { RackView } from '#lib/components/devices/view.js';

export class ListDecor {
	sparklines = $state<Map<TargetId, Serie[]>>(new Map());
	/** Last 24 hours per monitor, one cell per hour: the table's bars. */
	river = $state<Map<TargetId, RiverCell[]>>(new Map());
	/** Security badges: empty while the score is off in Settings, or for unrated kinds. */
	security = $state<Map<TargetId, { grade: SecurityGrade | null; score: number | null }>>(new Map());
	kindLabels = $state<Map<string, string>>(new Map());
	/** Option keys per device type: what is not an option of its type is a free tag. */
	optionKeys = $state<Map<string, Set<string>>>(new Map());

	private targets: () => Target[];
	private view: () => RackView;

	constructor(targets: () => Target[], view: () => RackView) {
		this.targets = targets;
		this.view = view;
	}

	private async load(signal?: AbortSignal) {
		// Decoration on top of the list the shared store already holds: a
		// failure here must not take the rack down with it.
		const next = await loadSparklines(24 * 3600, signal).catch(() => null);
		if (next) this.sparklines = next;
		return next !== null;
	}

	/** The table's 24 hourly segments: one aggregated query for the whole fleet. */
	private async loadBars(signal?: AbortSignal) {
		// The monitors tell a device's silent hour (down) from an empty one; read
		// untracked, so that each refresh of the store does not restart the timer.
		const monitors = untrack(() => this.targets());
		const cells = await loadRiver(DAY_SECONDS, signal, 24, monitors).catch(() => null);
		if (cells) this.river = cells;
		return cells !== null;
	}

	private async loadSecurity(signal?: AbortSignal) {
		// Decoration too: no badge rather than an error when it cannot be read.
		const summary = await getSecuritySummary(signal).catch(() => null);
		if (!summary) return false;
		this.security = new Map(
			summary.enabled ? summary.devices.map((d) => [d.target_id, { grade: d.grade, score: d.score }]) : []
		);
	}

	private async loadKinds(signal?: AbortSignal) {
		try {
			const collectors = await listCollectors(signal);
			this.kindLabels = new Map(collectors.map((c) => [c.kind, kindLabel(c.kind, c.label)]));
			this.optionKeys = new Map(collectors.map((c) => [c.kind, new Set(c.options.map((o) => o.key))]));
		} catch {
			// The raw kind is a fine label until the server describes it.
		}
	}

	/** Kind labels, then the security scores (they move slowly: recomputed every 15 min server-side). */
	listenStatic() {
		const controller = new AbortController();
		void this.loadKinds(controller.signal);
		const stopSecurity = startPolling((signal) => this.loadSecurity(signal), 120_000);
		return () => {
			controller.abort();
			stopSecurity();
		};
	}

	// The "Last 24 h" bars are drawn by the compact table (the default): they
	// were once fetched with the sparklines below, which only the tiles load,
	// so the table never received any. One `min by (target)` query, refreshed
	// every minute (a slot is an hour).
	listenBars() {
		if (this.view() !== 'compact') return;
		return startPolling((signal) => this.loadBars(signal), 60_000);
	}

	// Sparklines are drawn by the tiles only: the compact list (the default)
	// never shows them, so it does not pay for a range query over every
	// device's series every 30 s — several megabytes with a few hundred devices.
	listenSparklines() {
		if (this.view() !== 'tiles') return;
		return startPolling((signal) => this.load(signal), 30_000);
	}
}
