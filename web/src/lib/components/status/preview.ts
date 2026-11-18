/**
 * The live preview of the status page editor: the public document
 * (`PublicStatus`) the page would serve, built from the editor's unsaved
 * state. What only the server knows — each service's state and history, the
 * announcements, the calendar — comes from the saved page's preview document
 * (`GET /api/status-pages/{id}/preview`), matched by monitor id; a monitor the
 * page does not show yet gets its current state from the monitor list and a
 * "no data" bar of the same length. Pure, so it is tested without a browser.
 */
import type {
	PublicCalendarDay,
	PublicDayBucket,
	PublicItemState,
	PublicOverall,
	PublicStatus,
	PublicStatusGroup,
	PublicStatusItem,
	StatusPageAccent,
	StatusPageTheme,
	StatusSubscriptionKind,
	Target
} from '#lib/api/index.js';
import type { DraftGroup } from './structure.ts';

export interface PreviewDraft {
	slug: string;
	title: string;
	description: string;
	theme: StatusPageTheme;
	showDays: number;
	accent: StatusPageAccent;
	showCalendar: boolean;
	showLatency: boolean;
	footerText: string;
	homepageUrl: string;
	/** What the page's logo will be once saved (a data URL while unsaved), `null` for none. */
	logoUrl: string | null;
	subscribeEmail: boolean;
	hideBranding: boolean;
	groups: DraftGroup[];
	/** Checked monitor folders (dynamic), in order. */
	folders: string[];
	/** Public names of the folder members, by monitor id. */
	folderLabels: Record<number, string>;
}

const DAY = 86_400;

/** Today's state of a monitor the saved page does not show yet, from the monitor list. */
export function targetState(target: Target | undefined): PublicItemState {
	if (!target || !target.enabled) return 'unknown';
	if (target.error_kind === 'down') return 'down';
	if (target.last_probe_at && !target.last_error) return 'up';
	return 'unknown';
}

/** Bars with nothing measured: the dates of `like` when known, else `count` bars ending at `now`. */
export function blankHistory(like: readonly PublicDayBucket[] | undefined, count: number, stepSecs: number, now: Date): PublicDayBucket[] {
	if (like && like.length > 0) return like.map((day) => ({ ...day, uptime_pct: null, down_minutes: null }));
	const end = Math.floor(now.getTime() / 1000 / stepSecs) * stepSecs;
	const out: PublicDayBucket[] = [];
	for (let i = count - 1; i >= 0; i -= 1) {
		const at = new Date((end - i * stepSecs) * 1000).toISOString();
		out.push({ date: stepSecs >= DAY ? at.slice(0, 10) : `${at.slice(0, 19)}Z`, uptime_pct: null, down_minutes: null, incidents: 0 });
	}
	return out;
}

function blankCalendar(now: Date): PublicCalendarDay[] {
	return blankHistory(undefined, 90, DAY, now).map((day) => ({ date: day.date, state: 'nodata', uptime_pct: null, incidents: 0 }));
}

/**
 * Services as the public page would list them: the groups of the editor, then
 * the current members of each checked folder (a folder member placed by hand
 * keeps its place), grouped by name in the order of first appearance — the
 * server's own rule (`merge_items`).
 */
export function previewGroups(draft: PreviewDraft, targets: readonly Target[]): { name: string; target_id: number; label: string }[][] {
	const byId = new Map(targets.map((t) => [t.id, t]));
	const rows: { name: string; target_id: number; label: string }[] = [];
	const seen = new Set<number>();
	for (const group of draft.groups) {
		for (const item of group.items) {
			const target = byId.get(item.target_id);
			if (!target || seen.has(item.target_id)) continue;
			seen.add(item.target_id);
			let name = group.name.trim();
			// An item without a group that sits in a checked folder shows under the folder.
			if (name === '' && target.group_name && draft.folders.includes(target.group_name)) name = target.group_name;
			rows.push({ name, target_id: item.target_id, label: item.label.trim() || target.name });
		}
	}
	const members = [...targets].sort((a, b) => a.position - b.position || a.id - b.id);
	for (const folder of draft.folders) {
		for (const target of members) {
			if (target.group_name !== folder || seen.has(target.id)) continue;
			seen.add(target.id);
			rows.push({ name: folder, target_id: target.id, label: draft.folderLabels[target.id]?.trim() || target.name });
		}
	}
	const groups: { name: string; target_id: number; label: string }[][] = [];
	for (const row of rows) {
		const group = groups.find((g) => g[0].name === row.name);
		if (group) group.push(row);
		else groups.push([row]);
	}
	return groups;
}

/**
 * The document of the preview. `base` is the saved page's preview document
 * (`null` for a page not created yet, or when it could not be read).
 */
export function buildPreview(draft: PreviewDraft, targets: readonly Target[], base: PublicStatus | null, now: Date = new Date()): PublicStatus {
	const known = new Map<number, PublicStatusItem>();
	for (const group of base?.groups ?? []) for (const item of group.items) if (item.target_id !== undefined) known.set(item.target_id, item);
	const sample = base?.groups[0]?.items[0]?.history;
	const byId = new Map(targets.map((t) => [t.id, t]));

	// Bars: the saved page's own period; for a new page, the chosen period.
	const stepSecs = base?.page.step_secs ?? (draft.showDays <= 1 ? 3600 : DAY);
	const windowSecs = base?.page.window_secs ?? Math.max(1, draft.showDays) * DAY;
	const historyDays = base?.page.history_days ?? Math.max(1, draft.showDays);
	const bars = Math.max(1, Math.round(windowSecs / stepSecs));

	const groups: PublicStatusGroup[] = previewGroups(draft, targets).map((rows) => ({
		name: rows[0].name,
		items: rows.map((row) => {
			const saved = known.get(row.target_id);
			const target = byId.get(row.target_id);
			const item: PublicStatusItem = saved
				? { ...saved, label: row.label }
				: {
						key: `preview-${row.target_id}`,
						target_id: row.target_id,
						label: row.label,
						state: targetState(target),
						uptime_24h: null,
						uptime_7d: null,
						uptime_30d: null,
						uptime_90d: null,
						uptime_history: null,
						latency_ms: null,
						history: blankHistory(sample, bars, stepSecs, now),
						external: target?.kind === 'provider' || undefined
					};
			if (!draft.showLatency) delete item.latency_series;
			return item;
		})
	}));

	const items = groups.flatMap((g) => g.items);
	const down = items.filter((i) => i.state === 'down').length;
	const degraded = items.filter((i) => i.state === 'degraded').length;
	const overall: PublicOverall =
		base?.overall === 'maintenance' ? 'maintenance' : down > 0 && down === items.length ? 'major' : down > 0 || degraded > 0 ? 'degraded' : 'operational';

	const kinds: StatusSubscriptionKind[] = (base?.page.subscribe_kinds ?? []).filter((kind) => kind !== 'email');
	if (draft.subscribeEmail) kinds.unshift('email');

	return {
		page: {
			slug: draft.slug,
			title: draft.title.trim(),
			description: draft.description.trim(),
			theme: draft.theme,
			show_uptime_days: draft.showDays,
			history_days: historyDays,
			window_secs: windowSecs,
			step_secs: stepSecs,
			updated_at: base?.page.updated_at ?? '',
			accent: draft.accent,
			scenes: [],
			scene_rotation: 'visit',
			simple: true,
			show_calendar: draft.showCalendar || undefined,
			show_latency: draft.showLatency || undefined,
			footer_text: draft.footerText.trim(),
			homepage_url: draft.homepageUrl.trim(),
			logo_url: draft.logoUrl,
			brand_name: base?.page.brand_name,
			subscribe: kinds.length > 0,
			subscribe_kinds: kinds,
			hide_branding: draft.hideBranding
		},
		overall,
		generated_at: base?.generated_at ?? now.toISOString(),
		groups,
		calendar: draft.showCalendar ? (base?.calendar ?? blankCalendar(now)) : undefined,
		incidents: base?.incidents ?? [],
		maintenance: base?.maintenance ?? []
	};
}
