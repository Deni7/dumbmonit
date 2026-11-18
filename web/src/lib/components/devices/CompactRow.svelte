<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * One monitor on a single line, for the list of /targets. Every piece of
	 * information has a column of its own, the same width on every row whether
	 * the monitor has an alert, a score or a site or not (an empty cell stays
	 * empty): icon, name, address, site, age, score, alerts, state. The score
	 * and the alert count therefore never move.
	 *
	 * The columns fold with the width of the list itself (a container query):
	 * wide, all of them; medium, the address goes under the name; narrow, the
	 * site joins that second line and the age and the score are dropped.
	 * Opened through a stretched link on the name, so the score and the alert
	 * count can link on their own.
	 *
	 * Not repeated here, because the icon, the section or the page already say
	 * it: the kind's name (the icon's tooltip), the word "Last seen", the
	 * address when it is the name, the site when the list is filtered to one.
	 */
	import type { Target } from '#lib/api/index.js';
	import type { TargetState } from '#lib/format.js';
	import { STATE_LABEL, formatAge, formatDateTime } from '#lib/format.js';
	import KindIcon from '#lib/components/KindIcon.svelte';
	import SecurityBadge from './SecurityBadge.svelte';
	import SiteBadge from './SiteBadge.svelte';
	import StatusText from './StatusText.svelte';
	import AlertBell from './AlertBell.svelte';
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
	}

	let { target, state, kindLabel, site = null, depth = 0, shadowed = false, security = null, alerts }: Props = $props();

	/** An address that only repeats the name tells nothing more. */
	const address = $derived(target.address.trim().toLowerCase() === target.name.trim().toLowerCase() ? '' : target.address);
</script>

<div
	class={`@container relative min-h-12 px-3 py-1.5 transition-colors hover:bg-surface-2 ${shadowed ? 'opacity-60' : ''}`}
>
	<div
		class="grid min-h-9 items-center gap-x-3 grid-cols-[1.5rem_minmax(0,1fr)_2.5rem_6.5rem] @xl:grid-cols-[1.5rem_minmax(0,1fr)_2rem_3rem_2.75rem_2.5rem_6.5rem] @3xl:grid-cols-[1.5rem_minmax(0,1.5fr)_minmax(0,1fr)_2rem_3rem_2.75rem_2.5rem_6.5rem]"
	>
		<span class="flex size-6 items-center justify-center text-ink-2" title={kindLabel ?? target.kind}>
			<KindIcon kind={target.kind} />
		</span>

		<span class="min-w-0" style={depth ? `padding-left: ${Math.min(depth, 3) * 1.25}rem` : undefined}>
			<a
				href={withBase(`/targets/${target.id}`)}
				class="block min-w-0 truncate text-[0.875rem] font-semibold text-ink after:absolute after:inset-0 after:content-[''] focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-signal"
				aria-label={`${target.name}, ${STATE_LABEL[state]}`}
			>{target.name}</a>
			<!-- Second line while the address has no column of its own (and the site, on a narrow list). -->
			{#if address || site}
				<span class="flex min-w-0 items-center gap-1.5 text-[0.75rem] text-ink-3 @3xl:hidden">
					{#if site}<SiteBadge code={site.code} name={site.name} class="@xl:hidden" />{/if}
					{#if address}<span class="truncate" title={address}>{address}</span>{/if}
				</span>
			{/if}
		</span>

		<span class="hidden min-w-0 truncate text-[0.8125rem] text-ink-3 @3xl:block" title={address || undefined}>{address}</span>

		<span class="hidden items-center justify-center @xl:flex">
			{#if site}<SiteBadge code={site.code} name={site.name} />{/if}
		</span>

		<span class="tnum hidden text-right text-[0.8125rem] text-ink-3 @xl:block" title={target.last_probe_at ? formatDateTime(target.last_probe_at) : undefined}>
			{formatAge(target.last_probe_at)}
		</span>

		<span class="hidden items-center justify-center @xl:flex">
			{#if security?.grade}
				<SecurityBadge quiet grade={security.grade} score={security.score} href={`/targets/${target.id}#security`} class="relative z-[1] -my-2 !min-w-10 justify-center" />
			{/if}
		</span>

		<span class="flex items-center justify-center">
			<AlertBell quiet targetId={target.id} counts={alerts} class="-my-2" />
		</span>

		<StatusText {state} title={target.last_error ?? undefined} />
	</div>
</div>
