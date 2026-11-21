<script lang="ts">
	/**
	 * One announcement — incident or maintenance window — with its timeline.
	 * The latest message comes first; the earlier ones fold under "Earlier
	 * updates" so a long incident does not push the services off screen. A
	 * resolved incident with a published post-mortem ends with it, folded:
	 * plain text by paragraphs, never markup.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import type { PublicIncident } from '#lib/api/index.js';
	import { formatDateTime, formatRelative } from '#lib/format.js';
	import { Plate, Tilt } from '#lib/ui/index.js';
	import { INCIDENT_STATUS, KIND_LABEL, isClosed } from './words';
	import { paragraphs } from './postmortem';

	interface Props {
		incident: PublicIncident;
		/** Compact: for the "Past incidents" list. */
		compact?: boolean;
	}

	let { incident, compact = false }: Props = $props();

	const status = $derived(INCIDENT_STATUS[incident.status] ?? INCIDENT_STATUS.investigating);
	const closed = $derived(isClosed(incident.status));
	// Newest first; the timeline reads top-down like a feed.
	const updates = $derived([...incident.updates].reverse());
	const latest = $derived(updates[0] ?? null);
	const earlier = $derived(updates.slice(1));
	const isMaintenance = $derived(incident.kind === 'maintenance');
	const postmortem = $derived(closed ? (incident.postmortem ?? null) : null);
	const sections = $derived(
		postmortem
			? [
					{ title: m.status_postmortem_public_summary(), text: postmortem.summary },
					{ title: m.status_postmortem_public_impact(), text: postmortem.impact },
					{ title: m.status_postmortem_public_root_cause(), text: postmortem.root_cause }
				].filter((section) => section.text.trim() !== '')
			: []
	);
</script>

<Tilt tag="article" maxTilt={2.5} class={`rounded-[var(--radius-card)] border bg-surface ${closed || compact ? 'border-line' : isMaintenance ? 'border-info/40' : incident.severity === 'major' ? 'border-warning/40' : 'border-advisory/40'} ${compact ? 'px-4 py-3' : 'px-4 py-4 shadow-lift sm:px-5'}`}>
	<div class="flex flex-wrap items-center gap-2">
		<Plate tone={isMaintenance ? 'info' : closed ? 'ghost' : incident.severity === 'major' ? 'warning' : 'advisory'} label={KIND_LABEL[incident.kind]} bare />
		<Plate tone={status.tone} label={status.label} />
		{#if !isMaintenance && !compact}
			<span class="text-[0.8125rem] text-ink-2">{incident.severity === 'major' ? m.status_incident_card_impact_major() : m.status_incident_card_impact_minor()}</span>
		{/if}
	</div>
	<h3 class={`mt-2 font-semibold tracking-tight text-ink ${compact ? 'text-base' : 'text-lg'}`}>{incident.title}</h3>
	<p class="mt-1 text-[0.8125rem] text-ink-2">
		{#if isMaintenance}
			<time class="tnum" datetime={incident.starts_at}>{formatDateTime(incident.starts_at)}</time>
			{#if incident.ends_at}
				→ <time class="tnum" datetime={incident.ends_at}>{formatDateTime(incident.ends_at)}</time>
			{/if}
		{:else}
			{m.status_incident_card_started()} <time class="tnum" datetime={incident.starts_at} title={formatDateTime(incident.starts_at)}>{formatRelative(incident.starts_at)}</time>
			{#if incident.ends_at}
				· {m.status_incident_card_resolved()} <time class="tnum" datetime={incident.ends_at} title={formatDateTime(incident.ends_at)}>{formatRelative(incident.ends_at)}</time>
			{/if}
		{/if}
	</p>

	{#if latest}
		<div class="mt-3 border-l-2 border-line pl-3">
			<p class="text-sm whitespace-pre-line text-ink">{latest.body}</p>
			<p class="mt-1 text-[0.75rem] text-ink-2">
				<span class="font-semibold">{INCIDENT_STATUS[latest.status]?.label ?? latest.status}</span>
				· <time class="tnum" datetime={latest.created_at} title={formatDateTime(latest.created_at)}>{formatRelative(latest.created_at)}</time>
			</p>
		</div>
	{/if}

	{#if earlier.length > 0}
		<details class="mt-2 group">
			<summary class="cursor-pointer text-[0.8125rem] text-ink-2 hover:text-ink">
				{m.status_incident_card_earlier({ count: earlier.length })}
			</summary>
			<ol class="mt-2 space-y-3 border-l-2 border-line pl-3">
				{#each earlier as update (update.created_at + update.status)}
					<li>
						<p class="text-sm whitespace-pre-line text-ink">{update.body}</p>
						<p class="mt-1 text-[0.75rem] text-ink-2">
							<span class="font-semibold">{INCIDENT_STATUS[update.status]?.label ?? update.status}</span>
							· <time class="tnum" datetime={update.created_at}>{formatDateTime(update.created_at)}</time>
						</p>
					</li>
				{/each}
			</ol>
		</details>
	{/if}

	{#if postmortem}
		<details class="mt-3 border-t border-line pt-3">
			<summary class="flex min-h-10 cursor-pointer items-center text-sm font-semibold text-ink hover:text-ink-2">
				{m.status_postmortem_public_heading()}
				{#if postmortem.published_at}
					<span class="ml-2 font-normal text-[0.8125rem] text-ink-2">
						{m.status_postmortem_public_published({ date: formatDateTime(postmortem.published_at) })}
					</span>
				{/if}
			</summary>
			<div class="mt-2 grid gap-4">
				{#each sections as section (section.title)}
					<section>
						<h4 class="text-[0.8125rem] font-semibold text-ink-2">{section.title}</h4>
						{#each paragraphs(section.text) as paragraph, index (index)}
							<p class="mt-1.5 max-w-prose text-sm whitespace-pre-line text-ink">{paragraph}</p>
						{/each}
					</section>
				{/each}
				{#if postmortem.timeline.length > 0}
					<section>
						<h4 class="text-[0.8125rem] font-semibold text-ink-2">{m.status_postmortem_public_timeline()}</h4>
						<ol class="mt-1.5 space-y-2 border-l-2 border-line pl-3">
							{#each postmortem.timeline as entry, index (index)}
								<li>
									<time class="tnum text-[0.75rem] text-ink-2" datetime={entry.at}>{formatDateTime(entry.at)}</time>
									<p class="text-sm whitespace-pre-line text-ink">{entry.text}</p>
								</li>
							{/each}
						</ol>
					</section>
				{/if}
				{#if postmortem.actions.length > 0}
					<section>
						<h4 class="text-[0.8125rem] font-semibold text-ink-2">{m.status_postmortem_public_actions()}</h4>
						<ul class="mt-1.5 space-y-1.5">
							{#each postmortem.actions as item, index (index)}
								<li class="flex flex-wrap items-baseline gap-x-2 text-sm text-ink">
									<Plate tone={item.done ? 'signal' : 'ghost'} label={item.done ? m.status_postmortem_public_done() : m.status_postmortem_public_planned()} />
									<span class="min-w-0 break-words">{item.text}</span>
								</li>
							{/each}
						</ul>
					</section>
				{/if}
			</div>
		</details>
	{/if}
</Tilt>
