<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * One monitor as a row of the table on /targets. Every column is the same
	 * width on every row whether the monitor has a score or a site or not (an
	 * empty cell stays empty):
	 * status (a marked circle, no word), name over its address and kind (the
	 * quiet second line of an Alerts row: `address · Kind`), site (only when
	 * the instance has several),
	 * the last 24 hours as 24 hourly segments, security grade, age of the last
	 * check. The alert count sits on the name line, right after the name, and
	 * so does the disclosure of a parent's sub-monitors (`nesting.ts`): a bare
	 * chevron and count, quiet like the bell. Under its parent, a sub-monitor
	 * shows only its own part of the name (`ownName`); the whole name stays in
	 * the title and for screen readers.
	 * `columns.ts` holds the grid shared with the header line, and which
	 * columns fold when the list is narrow.
	 * Opened through a stretched link on the name, so the security grade, the
	 * alert count and the disclosure act on their own.
	 */
	import type { Target } from '#lib/api/index.js';
	import type { TargetState } from '#lib/format.js';
	import { STATE_LABEL, STATE_TONE, formatAge, formatDateTime } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import SecurityBadge from './SecurityBadge.svelte';
	import SiteBadge from './SiteBadge.svelte';
	import { StatusMark } from '#lib/ui/index.js';
	import AlertBell from './AlertBell.svelte';
	import { AlertTriangle, ChevronRight } from 'lucide-svelte';
	import { ROW_HEIGHT, SHOW, STATE_GLYPH, gridFor } from './columns';
	import { kindTag } from './kindTag';
	import { ownName } from './nesting';
	import type { RiverCell } from '#lib/components/enterprise/fleet.js';
	import type { TargetAlertCounts } from '#lib/alerts-attention.js';
	import type { SecurityGrade } from '#lib/api/index.js';

	interface Props {
		target: Target;
		state: TargetState;
		kindLabel?: string;
		/** The location this monitor sits at; no badge without one. */
		site?: { code: string; name: string } | null;
		/** The table has a site column (several locations); without it the site track is gone. */
		siteColumn?: boolean;
		depth?: number;
		/** The parent's name, so a sub-monitor under it shows only its own part. */
		parentName?: string | null;
		shadowed?: boolean;
		/** Security grade, when the score is on and this kind is rated. */
		security?: { grade: SecurityGrade | null; score: number | null } | null;
		/** Active alerts of this monitor, after its name; nothing without any. */
		alerts?: TargetAlertCounts;
		/** The last 24 hours, one cell per hour, oldest first; grey cells without data. */
		river?: RiverCell[] | null;
		/** Direct sub-monitors listed under this one; no disclosure without any. */
		childCount?: number;
		/** Sub-monitors below, at any depth, that need attention. */
		trouble?: number;
		/** The sub-monitors are shown. */
		open?: boolean;
		/** Opens or closes the sub-monitors. */
		ontoggle?: () => void;
	}

	let {
		target,
		state,
		kindLabel,
		site = null,
		siteColumn = true,
		depth = 0,
		parentName = null,
		shadowed = false,
		security = null,
		alerts,
		river = null,
		childCount = 0,
		trouble = 0,
		open = false,
		ontoggle
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
	const toggleLabel = $derived(
		trouble > 0
			? m.devices_rack_children_trouble({ name: target.name, count: childCount, trouble })
			: m.devices_rack_children({ name: target.name, count: childCount })
	);
	/** A closed parent with trouble below says so, by an icon as well as the tone. */
	const hiddenTrouble = $derived(!open && trouble > 0);
	/**
	 * An address that only repeats the name tells nothing more, and an agent's
	 * (or its services') is its identity, not where it is reached.
	 */
	const address = $derived(
		target.kind === 'agent' || target.kind === 'agent_service' || target.address.trim().toLowerCase() === target.name.trim().toLowerCase()
			? ''
			: target.address
	);
	const kind = $derived(kindLabel ?? target.kind);
	const tag = $derived(kindTag(target, kind));
	const grid = $derived(gridFor(siteColumn));
	/** Right under its parent, the parent's name is not repeated. */
	const shownName = $derived(depth > 0 ? ownName(target.name, parentName) : target.name);
	/** The kind says nothing more when the name already is the kind ("Databases" under its machine). */
	const showTag = $derived(String(tag).trim().toLowerCase() !== shownName.trim().toLowerCase());
</script>

<div class={`relative transition-colors hover:bg-surface-2/60 ${shadowed ? 'opacity-60' : ''}`}>
	<div class={`${grid} ${ROW_HEIGHT}`}>
		<span class="flex items-center justify-center">
			<StatusMark glyph={STATE_GLYPH[state]} tone={STATE_TONE[state]} label={STATE_LABEL[state]} title={target.last_error ?? STATE_LABEL[state]} />
		</span>

		<span class="min-w-0" style={depth ? `padding-left: ${Math.min(depth, 3) * 1.25}rem` : undefined}>
			<!-- The name line: the name, then its alert count and its sub-monitors, inline. -->
			<span class="flex h-5 min-w-0 items-center gap-1">
				<a
					href={withBase(`/targets/${target.id}`)}
					class="min-w-0 truncate text-sm font-semibold leading-5 text-ink after:absolute after:inset-0 after:content-[''] focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-signal"
					aria-label={`${target.name}, ${STATE_LABEL[state]}`}
					title={shownName !== target.name ? target.name : undefined}
				>{shownName}</a>
				<!-- 40 px hit areas, pulled into the 20 px line so the row keeps its height. -->
				<AlertBell quiet targetId={target.id} counts={alerts} class="-my-2.5" />
				{#if childCount > 0 && ontoggle}
					<button
						type="button"
						class="group relative z-[1] -my-2.5 inline-flex min-h-10 min-w-10 shrink-0 items-center justify-center rounded-[var(--radius-plate)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal"
						aria-expanded={open}
						aria-label={toggleLabel}
						title={toggleLabel}
						onclick={ontoggle}
						onpointerdown={(e) => e.stopPropagation()}
						onkeydown={(e) => e.stopPropagation()}
					>
						<!-- Bare, as the bell beside it: the chevron of the group headers and the count; a hairline only on hover. -->
						<span
							class={`tnum inline-flex h-6 items-center gap-0.5 rounded-[var(--radius-plate)] border border-transparent pr-1.5 pl-0.5 text-[0.75rem] font-semibold leading-none transition-colors group-hover:border-line-strong ${hiddenTrouble ? 'text-warning-ink' : 'text-ink-3 group-hover:text-ink-2'}`}
						>
							<ChevronRight class={`size-4 shrink-0 transition-transform duration-200 motion-reduce:transition-none ${open ? 'rotate-90' : ''}`} aria-hidden="true" />
							{#if hiddenTrouble}<AlertTriangle class="size-3 shrink-0" aria-hidden="true" />{/if}
							<span aria-hidden="true">{childCount}</span>
						</span>
					</button>
				{/if}
			</span>
			<!-- The quiet line of an Alerts row: the address first (it gives way first when the line is short), a dot, then the kind. -->
			<span class="mt-0.5 flex h-5 min-w-0 items-center gap-1.5 text-[0.8125rem] leading-5 text-ink-2">
				{#if site}<SiteBadge code={site.code} name={site.name} class={siteColumn ? '@xl:hidden' : ''} />{/if}
				{#if address}
					<span class="min-w-0 truncate" title={address}>{address}</span>
					{#if showTag}<span class="shrink-0 text-ink-3" aria-hidden="true">·</span>{/if}
				{/if}
				{#if showTag}<span class="max-w-40 shrink-0 truncate text-ink-3" title={kind}>{tag}</span>{/if}
			</span>
		</span>

		{#if siteColumn}
			<span class={`${SHOW.site} items-center justify-center`}>
				{#if site}<SiteBadge code={site.code} name={site.name} />{/if}
			</span>
		{/if}

		<span class={`${SHOW.river} items-center`}>
			<span class="flex h-6 w-full items-stretch gap-px" role="img" aria-label={riverLabel} title={riverLabel}>
				{#each cells as cell, i (i)}
					<span class={`min-w-0 flex-1 rounded-[1px] ${CELL[cell]} ${cell === 'none' ? 'opacity-70' : ''}`}></span>
				{/each}
			</span>
		</span>

		<span class={`${SHOW.security} items-center justify-center`}>
			{#if security?.grade}
				<SecurityBadge quiet grade={security.grade} score={security.score} href={`/targets/${target.id}#security`} class="relative z-[1] !min-w-10 justify-center" />
			{/if}
		</span>

		<span class="tnum truncate text-right text-[0.75rem] text-ink-3" title={target.last_probe_at ? formatDateTime(target.last_probe_at) : undefined}>
			{formatAge(target.last_probe_at)}
		</span>
	</div>
</div>
