<script lang="ts">
	/** Identity of a monitor in its header band: state LED, name, type, address, tags, plates and last probe. */
	import type { ComponentProps } from 'svelte';
	import { ArrowUpRight } from 'lucide-svelte';
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';
	import type { SecurityReport as SecurityReportData, Target } from '#lib/api/index.js';
	import {
		displayState,
		formatDateTime,
		formatDuration,
		formatFailureReason,
		formatRelative,
		STATE_LABEL
	} from '#lib/format.js';
	import type { UptimeSummary } from '#lib/metrics.js';
	import { tagChip } from '#lib/tag-chip.js';
	import { Led, Plate, Tag, type Tone } from '#lib/ui/index.js';
	import KindIcon from '#lib/components/KindIcon.svelte';
	import CopyAddress from '#lib/components/devices/CopyAddress.svelte';
	import AlertBell from '#lib/components/devices/AlertBell.svelte';
	import SecurityBadge from '#lib/components/devices/SecurityBadge.svelte';

	interface Props {
		target: Target;
		id: number;
		parent: Target | null;
		/** The relay agent that probes this device, when it is not the server. */
		relay: Target | null;
		tone: Tone;
		blink: boolean;
		stateNow: ReturnType<typeof displayState>;
		kindLabel: string;
		identity: string;
		tagChips: ReturnType<typeof tagChip>[];
		certificate: { tone: Tone; label: string } | null;
		summary: UptimeSummary | null;
		securityShown: SecurityReportData | null;
		securityOpen: boolean;
		headerAlertCounts: ComponentProps<typeof AlertBell>['counts'];
	}

	let {
		target,
		id,
		parent,
		relay,
		tone,
		blink,
		stateNow,
		kindLabel,
		identity,
		tagChips,
		certificate,
		summary,
		securityShown,
		securityOpen = $bindable(),
		headerAlertCounts
	}: Props = $props();
</script>

<div class="flex min-w-0 gap-4">
	<Led {tone} {blink} size="lg" class="mt-2.5" label={STATE_LABEL[stateNow]} />
	<div class="min-w-0 flex-1">
		<h1 id="device-name" class="display text-xl break-words text-ink sm:text-3xl">{target.name}</h1>
		<!-- Quiet identity line: what it is, where it answers, how often, behind whom. -->
		<div class="mt-2 flex flex-wrap items-center gap-x-2.5 gap-y-1 text-sm text-ink-2">
			<span class="inline-flex items-center gap-1.5">
				<KindIcon kind={target.kind} class="size-4 text-ink-3" />
				<span class="label-tape">{kindLabel}</span>
			</span>
			{#if identity}
				<span class="text-ink-3" aria-hidden="true">·</span>
				<span class="tnum break-all" title={target.address}>{identity}</span>
				<CopyAddress value={identity} class="-mx-1.5" />
			{/if}
			<span class="text-ink-3" aria-hidden="true">·</span>
			<span class="tnum text-ink-3">{m.devices_target_every({ interval: formatDuration(target.interval_secs) })}</span>
			{#if target.parent_id !== null}
				<span class="text-ink-3" aria-hidden="true">·</span>
				<a href={withBase(`/targets/${target.parent_id}`)} class="inline-flex items-center gap-1 hover:text-ink hover:underline">
					{m.devices_target_behind({ name: parent?.name ?? m.devices_target_behind_unknown({ id: target.parent_id }) })}
					<ArrowUpRight class="size-3.5" aria-hidden="true" />
				</a>
			{/if}
			{#if target.via_agent !== null}
				<span class="text-ink-3" aria-hidden="true">·</span>
				<a
					href={withBase(`/targets/${target.via_agent}`)}
					class="inline-flex items-center gap-1 hover:text-ink hover:underline"
					title={m.devices_target_via_title()}
				>
					{m.devices_target_via({ name: relay?.name ?? m.devices_target_via_unknown({ id: target.via_agent }) })}
					<ArrowUpRight class="size-3.5" aria-hidden="true" />
				</a>
			{/if}
		</div>
		{#if tagChips.length > 0}
			<ul class="mt-2 flex flex-wrap items-center gap-1.5" aria-label={m.w1monitors_detail_tags_aria()}>
				{#each tagChips as chip (chip.label)}
					<li><Tag title={chip.title} class="!max-w-64">{chip.label}</Tag></li>
				{/each}
			</ul>
		{/if}
		<div class="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1.5">
			<Plate {tone} label={STATE_LABEL[stateNow]} size="md" pulse={blink} />
			{#if certificate}
				<Plate tone={certificate.tone} label={certificate.label} size="md" title={summary?.tlsVersion ?? undefined} />
			{/if}
			{#if securityShown}
				<SecurityBadge
					size="md"
					grade={securityShown.grade}
					score={securityShown.score}
					expanded={securityOpen}
					controls="security-report"
					onclick={() => (securityOpen = !securityOpen)}
					class="-my-2"
				/>
			{/if}
			<AlertBell targetId={id} counts={headerAlertCounts} class="-my-2" />
			<p class="tnum min-w-0 text-sm text-ink-2" title={formatDateTime(target.last_probe_at)}>
				{#if target.last_error}
					<span class={`break-words ${target.error_kind === 'config' ? 'text-advisory-ink' : 'text-warning-ink'}`}>{target.last_error}</span>
				{:else if stateNow === 'down'}
					<span class="text-warning-ink">{formatFailureReason(summary?.status?.reason)}</span>
					<span class="text-ink-3" aria-hidden="true">·</span>
					{m.devices_target_last_seen({ when: formatRelative(target.last_probe_at) })}
				{:else}
					{m.devices_target_last_seen({ when: formatRelative(target.last_probe_at) })}
				{/if}
			</p>
		</div>
	</div>
</div>
