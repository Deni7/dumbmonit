<script lang="ts">
	/**
	 * One compact line under the verdict: the incident or maintenance published
	 * on the status pages that is in progress, or a maintenance window covering
	 * now. Nothing shows (and nothing is said) when there is none, or when the
	 * lists cannot be read.
	 */
	import { Megaphone, Wrench } from 'lucide-svelte';
	import { listIncidents, listSilences } from '#lib/api/index.js';
	import type { Incident, Silence } from '#lib/api/types.js';
	import { parseServerDate } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import { startPolling } from '#lib/poll.js';
	import { wallNotices } from './wallNotice.js';

	interface Props {
		now: Date;
	}
	let { now }: Props = $props();

	let incidents = $state<Incident[]>([]);
	let silences = $state<Silence[]>([]);

	$effect(() =>
		startPolling(async (signal) => {
			const [a, b] = await Promise.allSettled([listIncidents(signal), listSilences(signal)]);
			if (a.status === 'fulfilled') incidents = a.value;
			if (b.status === 'fulfilled') silences = b.value;
			return a.status === 'fulfilled' || b.status === 'fulfilled';
		}, 60_000)
	);

	const notices = $derived(wallNotices(incidents, silences, now.getTime()));
	const first = $derived(notices[0] ?? null);
	const text = $derived.by(() => {
		if (!first) return '';
		if (first.kind === 'incident') return m.w1wall_notice_incident({ title: first.title });
		const end = parseServerDate(first.until);
		return end
			? m.w1wall_notice_maintenance_until({ title: first.title, time: end.toLocaleTimeString(getLocale(), { hour: '2-digit', minute: '2-digit' }) })
			: m.w1wall_notice_maintenance({ title: first.title });
	});
</script>

{#if first}
	<p
		class="notice mt-3 inline-flex max-w-full items-center gap-2 rounded-lg border border-line-strong bg-surface/90 px-3 py-1.5 text-[0.9375rem] font-medium text-ink shadow-lift"
		role="status"
		data-wall-info
	>
		{#if first.kind === 'incident'}
			<Megaphone class="size-4 shrink-0 text-advisory-ink" aria-hidden="true" />
		{:else}
			<Wrench class="size-4 shrink-0 text-info-ink" aria-hidden="true" />
		{/if}
		<span class="min-w-0 truncate">{text}</span>
		{#if notices.length > 1}
			<span class="shrink-0 text-ink-3">{m.w1wall_notice_more({ count: notices.length - 1 })}</span>
		{/if}
	</p>
{/if}
