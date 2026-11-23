<script lang="ts">
	import { withBase } from '#lib/paths.js';
	/**
	 * The service level objectives: for each, what it attained over its window
	 * against its target, the error budget left (a bar that empties) and, when
	 * the budget burns too fast, which burn. Read every five minutes.
	 */
	import { listSlos, type Slo } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { startPolling } from '#lib/poll.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { ErrorNotice, Plate, Skeleton } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { availabilityText, barWidth, budgetTone } from '../parts/format.js';

	type Span = 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12;
	let { span = 3 }: { span?: Span } = $props();

	const REFRESH_MS = 5 * 60_000;
	/** Objectives listed; the rest is on Status › SLOs. */
	const LIMIT = 5;

	let slos = $state<Slo[] | null>(null);
	let error = $state<unknown>(null);

	$effect(() =>
		startPolling(async (signal) => {
			try {
				slos = await listSlos(signal);
				error = null;
				return true;
			} catch (cause) {
				if (signal.aborted) return true;
				error = cause;
				return false;
			}
		}, REFRESH_MS)
	);

	const enabled = $derived((slos ?? []).filter((s) => s.enabled));
	/** The ones in trouble first: fast burn, slow burn, then the least budget left. */
	const shown = $derived(
		[...enabled]
			.sort((a, b) => {
				const burn = (s: Slo) => (s.status.verdict === 'fast' ? 0 : s.status.verdict === 'slow' ? 1 : 2);
				const left = (s: Slo) => s.status.budget?.remaining_percent ?? 101;
				return burn(a) - burn(b) || left(a) - left(b) || a.name.localeCompare(b.name);
			})
			.slice(0, LIMIT)
	);
	/** The window when every objective shares one ("30 days"). */
	const windowDays = $derived(enabled.length > 0 && enabled.every((s) => s.window_days === enabled[0].window_days) ? enabled[0].window_days : null);
</script>

<WidgetCard title={m.w9dash_slo_title()} sub={windowDays ? m.w9dash_slo_window({ days: windowDays }) : undefined} {span}>
	{#if slos === null && !error}
		<Skeleton class="h-9 w-full" rows={3} />
	{:else if slos === null}
		<ErrorNotice {error} title={m.w9dash_slo_error()} compact />
	{:else if shown.length === 0}
		<p class="m-0 py-2 text-sm text-ink-2">{m.w9dash_slo_empty()}</p>
		{#if auth.canOperate}
			<a href={withBase('/status?tab=slo')} class="inline-flex min-h-10 items-center text-sm font-medium text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink">{m.w9dash_slo_create()}</a>
		{/if}
	{:else}
		<ul class="m-0 list-none p-0">
			{#each shown as slo, i (slo.id)}
				{@const left = slo.status.budget?.remaining_percent ?? null}
				{@const tone = budgetTone(left)}
				<li class={`py-2 ${i > 0 ? 'border-t border-line' : ''}`}>
					<div class="flex items-baseline justify-between gap-2">
						<a href={withBase(`/status?tab=slo&slo=${slo.id}`)} class="min-w-0 truncate text-sm font-semibold text-ink hover:underline">{slo.name}</a>
						<span class="shrink-0 font-mono text-sm font-medium text-ink tabular-nums">
							{slo.status.measured && slo.status.attained !== null ? `${availabilityText(slo.status.attained)}%` : '—'}
						</span>
					</div>
					<div class="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 text-xs text-ink-2">
						<span>{m.w9dash_slo_target({ target: slo.objective })}</span>
						{#if slo.status.verdict !== 'none'}
							<Plate tone={slo.status.verdict === 'fast' ? 'warning' : 'advisory'} label={slo.status.verdict === 'fast' ? m.w9dash_slo_fast() : m.w9dash_slo_slow()} />
						{:else if left !== null}
							<span class={tone === 'ok' ? '' : 'font-semibold text-advisory-ink'}>
								{tone === 'spent' ? m.w9dash_slo_spent() : m.w9dash_slo_left({ percent: Math.round(left) })}
							</span>
						{:else if !slo.status.measured}
							<span>{m.w9dash_slo_unmeasured()}</span>
						{/if}
					</div>
					{#if left !== null}
						<div class="mt-1 h-1.5 overflow-hidden rounded-full bg-ghost" aria-hidden="true">
							<span class={`block h-full rounded-full ${tone === 'ok' ? 'bg-signal' : tone === 'low' ? 'bg-advisory' : 'bg-warning'}`} style={`width:${barWidth(left)}%`}></span>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet foot()}
		<a href={withBase('/status?tab=slo')}>{enabled.length > LIMIT ? m.w9dash_slo_more({ count: enabled.length }) : m.w9dash_slo_all()}</a>
	{/snippet}
</WidgetCard>
