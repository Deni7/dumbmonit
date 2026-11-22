<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	/**
	 * The maintenance windows that cover this monitor, active now or coming up:
	 * its own, the ones on every monitor, the ones matching its tags. Nothing
	 * (not even a heading) when there is none; a failed read hides it too, the
	 * page has its own errors. Managing them stays in Alerts.
	 */
	import { CalendarClock } from 'lucide-svelte';
	import { listSilences, type Silence, type Target } from '#lib/api/index.js';
	import { formatDateTime } from '#lib/format.js';
	import { scheduleLabel } from '#lib/components/alerts/helpers.js';
	import { Plate } from '#lib/ui/index.js';
	import { windowsFor } from './maintenance';

	interface Props {
		target: Target;
		/** Bumped by the page on each refresh. */
		refreshKey?: number;
		class?: string;
	}

	let { target, refreshKey = 0, class: className = '' }: Props = $props();

	let silences = $state<Silence[]>([]);
	$effect(() => {
		void target.id;
		void refreshKey;
		const controller = new AbortController();
		listSilences(controller.signal)
			.then((list) => (silences = list))
			.catch(() => {});
		return () => controller.abort();
	});

	const windows = $derived(windowsFor(silences, target));

	function scope(silence: Silence): string {
		return silence.target_id === null ? m.w1monitors_detail_maint_scope_all() : m.w1monitors_detail_maint_scope_this();
	}
	function timing(silence: Silence): string {
		if (silence.schedule.kind === 'recovery') return m.devices_silence_until_recovery();
		if (silence.active_now) {
			return silence.active_until ? m.w1monitors_detail_maint_active({ when: formatDateTime(silence.active_until) }) : scheduleLabel(silence.schedule);
		}
		return m.w1monitors_detail_maint_next({ when: formatDateTime(silence.next_start_at) });
	}
</script>

{#if windows.length > 0}
	<section class={`rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3 sm:px-6 ${className}`} aria-labelledby="maintenance-title">
		<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
			<h2 id="maintenance-title" class="flex items-center gap-2 text-sm font-semibold text-ink">
				<CalendarClock class="size-4 text-ink-3" aria-hidden="true" />
				{m.w1monitors_detail_maint_title()}
			</h2>
			<a href={withBase('/alerts?tab=now&maintenance=1')} class="inline-flex min-h-10 items-center text-sm text-ink-2 hover:text-ink hover:underline sm:min-h-0">{m.w1monitors_detail_maint_manage()}</a>
		</div>
		<ul class="mt-1 flex flex-col divide-y divide-line">
			{#each windows as silence (silence.id)}
				<li class="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-sm">
					<Plate tone={silence.active_now ? 'advisory' : 'ghost'} label={silence.active_now ? m.w1monitors_detail_maint_state_active() : m.w1monitors_detail_maint_state_upcoming()} />
					<span class="min-w-0 break-words font-medium text-ink">{silence.name}</span>
					<span class="text-ink-3" aria-hidden="true">·</span>
					<span class="text-ink-2">{scope(silence)}</span>
					<span class="text-ink-3" aria-hidden="true">·</span>
					<span class="tnum text-ink-2">{timing(silence)}</span>
				</li>
			{/each}
		</ul>
	</section>
{/if}
