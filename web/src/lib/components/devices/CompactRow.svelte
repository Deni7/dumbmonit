<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * One monitor as a row of the table on /targets. Every piece of information
	 * has a column of its own, the same width on every row whether the monitor
	 * has an alert, a score or a site or not (an empty cell stays empty):
	 * status (a LED and the state word), name over its kind, address, site,
	 * the last 24 hours as 24 hourly segments, response time, security grade,
	 * age of the last check, alerts. `columns.ts` holds the grid shared with
	 * the header line, and which columns fold when the list is narrow.
	 * Opened through a stretched link on the name, so the security grade and
	 * the alert count can link on their own.
	 */
	import type { Target } from '#lib/api/index.js';
	import type { TargetState } from '#lib/format.js';
	import { STATE_LABEL, formatAge, formatDateTime, formatLatency } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import SecurityBadge from './SecurityBadge.svelte';
	import SiteBadge from './SiteBadge.svelte';
	import StatusText from './StatusText.svelte';
	import AlertBell from './AlertBell.svelte';
	import { GRID, SHOW } from './columns';
	import type { RiverCell } from '#lib/components/enterprise/fleet.js';
	import type { TargetAlertCounts } from '#lib/alerts-attention.js';
	import type { SecurityGrade } from '#lib/api/index.js';

	interface Props {
		target: Target;
		state: TargetState;
		kindLabel?: string;
		/** The location this monitor sits at; no badge without one. */
		site?: { code: string; name: string } | null;
		depth?: number;
		shadowed?: boolean;
		/** Security grade, when the score is on and this kind is rated. */
		security?: { grade: SecurityGrade | null; score: number | null } | null;
		/** Active alerts of this monitor; an empty cell without any. */
		alerts?: TargetAlertCounts;
		/** The last 24 hours, one cell per hour, oldest first; grey cells without data. */
		river?: RiverCell[] | null;
		/** Latest response time in milliseconds, when the monitor measures one. */
		latencyMs?: number | null;
	}

	let {
		target,
		state,
		kindLabel,
		site = null,
		depth = 0,
		shadowed = false,
		security = null,
		alerts,
		river = null,
		latencyMs = null
	}: Props = $props();

	const SLOTS = 24;
	const CELL: Record<RiverCell, string> = {
		up: 'bg-signal',
		warn: 'bg-advisory',
		down: 'bg-warning',
		none: 'bg-ghost'
	};

	const cells = $derived<RiverCell[]>(
		river && river.length > 0 ? river.slice(-SLOTS) : Array.from({ length: SLOTS }, () => 'none' as const)
	);
	const riverLabel = $derived(
		m.devices_uptime_aria({
			up: cells.filter((c) => c === 'up').length,
			down: cells.filter((c) => c === 'down' || c === 'warn').length,
			none: cells.filter((c) => c === 'none').length
		})
	);
	/** A failed or paused monitor has no current response time worth showing. */
	const showLatency = $derived(
		latencyMs !== null && (state === 'online' || state === 'pending' || state === 'unknown')
	);
	/** An address that only repeats the name tells nothing more. */
	const address = $derived(target.address.trim().toLowerCase() === target.name.trim().toLowerCase() ? '' : target.address);
	const kind = $derived(kindLabel ?? target.kind);
</script>

<div class={`relative min-h-[3.25rem] py-1.5 transition-colors hover:bg-surface-2 ${shadowed ? 'opacity-60' : ''}`}>
	<div class={`${GRID} min-h-10`}>
		<StatusText {state} title={target.last_error ?? undefined} />

		<span class="min-w-0" style={depth ? `padding-left: ${Math.min(depth, 3) * 1.25}rem` : undefined}>
			<a
				href={withBase(`/targets/${target.id}`)}
				class="block min-w-0 truncate text-[0.875rem] font-semibold text-ink after:absolute after:inset-0 after:content-[''] focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-signal"
				aria-label={`${target.name}, ${STATE_LABEL[state]}`}
				title={address || undefined}
			>{target.name}</a>
			<span class="flex min-w-0 items-center gap-1.5 text-[0.75rem] text-ink-3">
				{#if site}<SiteBadge code={site.code} name={site.name} class="@xl:hidden" />{/if}
				<span class="truncate">{kind}</span>
			</span>
		</span>

		<span class={`${SHOW.address} min-w-0 truncate font-mono text-[0.75rem] text-ink-2`} title={address || undefined}>{address}</span>

		<span class={`${SHOW.site} items-center justify-center`}>
			{#if site}<SiteBadge code={site.code} name={site.name} />{/if}
		</span>

		<span class={`${SHOW.river} items-center`}>
			<span class="flex h-5 w-full items-stretch gap-px" role="img" aria-label={riverLabel} title={riverLabel}>
				{#each cells as cell, i (i)}
					<span class={`min-w-0 flex-1 rounded-[1px] ${CELL[cell]} ${cell === 'none' ? 'opacity-70' : ''}`}></span>
				{/each}
			</span>
		</span>

		<span class={`${SHOW.response} tnum text-right font-mono text-[0.75rem] text-ink-2`}>
			{showLatency ? formatLatency((latencyMs ?? 0) / 1000) : '—'}
		</span>

		<span class={`${SHOW.security} items-center justify-center`}>
			{#if security?.grade}
				<SecurityBadge quiet grade={security.grade} score={security.score} href={`/targets/${target.id}#security`} class="relative z-[1] -my-2 !min-w-10 justify-center" />
			{/if}
		</span>

		<span class="tnum text-right text-[0.8125rem] text-ink-3" title={target.last_probe_at ? formatDateTime(target.last_probe_at) : undefined}>
			{formatAge(target.last_probe_at)}
		</span>

		<span class="flex items-center justify-center">
			<AlertBell quiet targetId={target.id} counts={alerts} class="-my-2" />
		</span>
	</div>
</div>
