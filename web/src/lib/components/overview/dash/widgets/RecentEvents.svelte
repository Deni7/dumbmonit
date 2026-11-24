<script lang="ts">
	/**
	 * Recent events, newest first: alerts firing, resolving and acknowledged,
	 * and the changes around them (agent installs, edits, maintenance). One
	 * row each: when, an icon in the event's tone (the alerts page's icons),
	 * what happened (the server's sentence). The time column is as wide as its
	 * longest entry ("Wed 10:19 PM"), the same for every row, so a time never
	 * runs into its icon. The panel scrolls, the title leads to the alert
	 * history. From the widgets store.
	 */
	import { AlertTriangle, CheckCheck, CircleAlert, CircleCheck, Info, Wrench } from 'lucide-svelte';
	import type { WidgetEvent } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { ErrorNotice, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import { EVENT_TONE, TONE_INK } from '../parts/plates.js';
	import { ROW, ROW_ICON, STATE_PAD, TEXT_BODY, TEXT_SIDE, TEXT_SUB } from '../parts/scale.js';
	import { clockText } from '../parts/words.js';

	let { limit = 20, class: cls = '' }: { limit?: number; class?: string } = $props();

	const ICON: Record<WidgetEvent['kind'], typeof Info> = {
		crit: AlertTriangle,
		warn: CircleAlert,
		ok: CircleCheck,
		info: Info,
		maint: Wrench,
		ack: CheckCheck
	};

	const events = $derived(widgetsStore.data?.events ?? null);
	const shown = $derived((events ?? []).slice(0, limit));
</script>

<WidgetCard title={m.w9dash_events_title()} href="/alerts?tab=history" class={cls}>
	{#if !events && widgetsStore.loading}
		<div class={`flex flex-col gap-2.5 border-t border-line ${STATE_PAD}`} aria-busy="true">
			<Skeleton class="h-4 w-full" rows={4} />
		</div>
	{:else if !events}
		<div class={`border-t border-line ${STATE_PAD}`}>
			<ErrorNotice error={widgetsStore.error} title={m.w9dash_common_error()} compact onretry={() => void widgetsStore.refresh()} />
		</div>
	{:else if shown.length === 0}
		<p class={`m-0 border-t border-line ${STATE_PAD} ${TEXT_SUB}`}>{m.w9dash_events_empty()}</p>
	{:else}
		<ul class="m-0 grid list-none grid-cols-[max-content_auto_minmax(0,1fr)] gap-x-3 p-0">
			<!-- Events can share a time: keyed by position (the list is redrawn whole on each poll). -->
			{#each shown as event, i (i)}
				{@const Icon = ICON[event.kind]}
				<li class={`col-span-3 grid grid-cols-subgrid items-center ${ROW}`}>
					<time class={`text-ink-2 ${TEXT_SIDE}`} datetime={event.at}>{clockText(event.at)}</time>
					<Icon class={`${ROW_ICON} ${TONE_INK[EVENT_TONE[event.kind]]}`} aria-hidden="true" />
					{#if event.target_id !== null}
						<a href={withBase(`/targets/${event.target_id}`)} class={`min-w-0 hover:underline ${TEXT_BODY}`}>{event.text}</a>
					{:else}
						<span class={`min-w-0 ${TEXT_BODY}`}>{event.text}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</WidgetCard>
