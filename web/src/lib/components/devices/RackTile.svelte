<script lang="ts">
	import { withBase } from '#lib/paths.js';
	import { viewport } from '#lib/stores/viewport.svelte.js';
	/**
	 * One monitor as a tile, for the tiles view of /targets: a plain bordered
	 * card like the rest of the app (no shadow, no light, no lift). Icon, name
	 * and site on top, the address under them, the state and the quiet
	 * indicators (alerts, score, age) on one line, the 24 h sparkline last.
	 * A child names the unit it sits behind instead of being indented.
	 * Opened through a stretched link on the name, so the score and the alert
	 * count can link on their own.
	 */
	import type { Target } from '#lib/api/index.js';
	import type { TargetState } from '#lib/format.js';
	import { STATE_LABEL, STATE_TONE, formatAge, formatDateTime } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import Chart, { type Serie } from '#lib/components/Chart.svelte';
	import KindIcon from '#lib/components/KindIcon.svelte';
	import { CornerDownRight } from 'lucide-svelte';
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
		sparkline?: Serie[] | null;
		/** Name of the unit this one sits behind, when it is a child. */
		parentName?: string | null;
		shadowed?: boolean;
		/** Security grade, when the score is on and this kind is rated. */
		security?: { grade: SecurityGrade | null; score: number | null } | null;
		/** Active alerts of this monitor; nothing without any. */
		alerts?: TargetAlertCounts;
	}

	let { target, state, kindLabel, site = null, sparkline = null, parentName = null, shadowed = false, security = null, alerts }: Props = $props();

	const tone = $derived(STATE_TONE[state]);
	// Phones: two small tiles a row — icon, name and the state. The rest
	// stays on the device page.
	const phone = $derived(viewport.phone);
	/** An address that only repeats the name tells nothing more. */
	const address = $derived(target.address.trim().toLowerCase() === target.name.trim().toLowerCase() ? '' : target.address);
</script>

<div
	class={`relative flex h-full flex-col rounded-[var(--radius-card)] border border-line bg-surface transition-colors hover:border-line-strong ${phone ? 'gap-2 p-3' : 'gap-2.5 p-3.5'} ${shadowed ? 'opacity-60' : ''}`}
>
	<div class="flex items-center gap-2">
		<span class="flex size-5 shrink-0 items-center justify-center text-ink-2" title={kindLabel ?? target.kind}>
			<KindIcon kind={target.kind} class="size-[1.125rem]" />
		</span>
		<a
			href={withBase(`/targets/${target.id}`)}
			class="min-w-0 flex-1 truncate text-[0.875rem] font-semibold text-ink after:absolute after:inset-0 after:rounded-[var(--radius-card)] after:content-[''] focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-signal"
			aria-label={`${target.name}, ${STATE_LABEL[state]}`}
		>{target.name}</a>
		{#if site}<SiteBadge code={site.code} name={site.name} />{/if}
	</div>

	{#if !phone}
		<div class="min-w-0 text-[0.8125rem] text-ink-3">
			{#if address}<p class="truncate" title={address}>{address}</p>{/if}
			{#if parentName}
				<p class="flex min-w-0 items-center gap-1">
					<CornerDownRight class="size-3.5 shrink-0" aria-hidden="true" />
					<span class="truncate">{m.devices_target_behind({ name: parentName })}</span>
				</p>
			{/if}
		</div>
	{/if}

	<div class="mt-auto flex min-w-0 items-center gap-x-2">
		<StatusText {state} title={target.last_error ?? undefined} class="flex-1" />
		<AlertBell quiet targetId={target.id} counts={alerts} class="-my-1.5" />
		{#if security?.grade && !phone}
			<SecurityBadge quiet grade={security.grade} score={security.score} href={`/targets/${target.id}#security`} class="relative z-[1] -my-1.5" />
		{/if}
		{#if !phone}
			<span class="tnum w-10 shrink-0 text-right text-[0.8125rem] text-ink-3" title={target.last_probe_at ? formatDateTime(target.last_probe_at) : undefined}>{formatAge(target.last_probe_at)}</span>
		{/if}
	</div>

	{#if !phone}
		<div class="h-8">
			{#if sparkline && sparkline.length > 0}
				<Chart series={sparkline} compact height={32} tone={tone === 'warning' ? 'warning' : tone === 'advisory' ? 'advisory' : tone === 'ghost' ? 'ghost' : 'signal'} />
			{/if}
		</div>
	{/if}
</div>
