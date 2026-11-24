<script lang="ts">
	/**
	 * Services, compact (Hybrid): the HTTP, DNS, TLS, SMTP… checks that run,
	 * worst first, one line each: a status mark (its shape says the state),
	 * the check and its kind, the last checks as beats (a short bar per
	 * success, a full-height one per failure, a hollow stub where there was no
	 * data), the 30-day availability and the p95. Paused checks fold into one
	 * line, "N paused", that opens on their names. From the dashboards' store.
	 * `fill`: the card takes its parent's height and scrolls inside.
	 */
	import { ChevronRight } from 'lucide-svelte';
	import type { WidgetStatus } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { StatusMark } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { checkSummary, worstFirst, type Span } from '../parts/sites.js';
	import { num, statusWord } from '../parts/labels.js';

	let { span = 6, fill = false, class: cls = '' }: { span?: Span; fill?: boolean; class?: string } = $props();

	const data = $derived(widgetsStore.data);
	const all = $derived(data?.services ?? []);
	const running = $derived(worstFirst(all.filter((s) => s.status !== 'paused')));
	const paused = $derived(all.filter((s) => s.status === 'paused'));
	let showPaused = $state(false);

	const MARK: Record<WidgetStatus, { glyph: 'none' | 'cross' | 'bang' | 'ring'; tone: 'signal' | 'advisory' | 'warning' | 'ghost' }> = {
		up: { glyph: 'none', tone: 'signal' },
		degraded: { glyph: 'bang', tone: 'advisory' },
		down: { glyph: 'cross', tone: 'warning' },
		paused: { glyph: 'ring', tone: 'ghost' }
	};

	function beatsLabel(checks: (boolean | null)[]): string {
		const { total, failed } = checkSummary(checks);
		return failed ? m.w9layout_services_beats_failed({ count: total, failed }) : m.w9layout_services_beats_ok({ count: total });
	}

	function availability(pct: number | null): string {
		if (pct === null || !Number.isFinite(pct)) return '—';
		return m.w9layout_pct({ value: pct >= 99.995 ? num(100) : num(Math.floor(pct * 100) / 100, 2) });
	}
</script>

{#snippet foot()}
	<a href={withBase('/targets')}>{m.w9layout_services_all()}</a>
{/snippet}

<WidgetCard {span} {fill} flush title={m.w9layout_services_title()} foot={all.length ? foot : undefined} class={cls}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={all.length === 0} emptyText={m.w9layout_services_empty()} class="px-4 pb-3" rows={3}>
		<ul class="m-0 list-none p-0 @container">
			{#each running as service (service.target_id)}
				{@const mark = MARK[service.status]}
				<li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 border-t border-line px-4 py-1 @sm:grid-cols-[minmax(0,10rem)_minmax(0,1fr)_auto]">
					<a href={withBase(`/targets/${service.target_id}`)} class="group flex min-h-10 min-w-0 items-center gap-2">
						<StatusMark glyph={mark.glyph} tone={mark.tone} label={statusWord(service.status)} />
						<span class="min-w-0">
							<span class="block truncate text-sm font-semibold text-ink group-hover:underline">{service.name}</span>
							<span class="block truncate text-xs text-ink-2">{service.kind}</span>
						</span>
					</a>
					<div class="order-last col-span-2 flex h-4 min-w-0 items-end gap-[2px] @sm:order-none @sm:col-span-1 @sm:h-5" role="img" aria-label={beatsLabel(service.checks)}>
						{#each service.checks as check, i (i)}
							{#if check === null}
								<i class="block h-[35%] min-w-[2px] flex-1 rounded-[2px] border border-line-strong"></i>
							{:else if check}
								<i class="block h-[60%] min-w-[2px] flex-1 rounded-[2px] bg-river-up"></i>
							{:else}
								<i class="block h-full min-w-[2px] flex-1 rounded-[2px] bg-warning"></i>
							{/if}
						{/each}
					</div>
					<span class="text-right font-mono text-[0.8125rem] whitespace-nowrap text-ink tabular-nums">
						{availability(service.availability_30d)}
						<span class="block text-xs text-ink-3">{service.p95_ms !== null ? m.w10hybrid_services_p95({ value: m.w9layout_ms({ value: num(service.p95_ms) }) }) : '—'}</span>
					</span>
				</li>
			{/each}
			{#if paused.length > 0}
				<li class="border-t border-line">
					<button
						type="button"
						class="flex min-h-10 w-full items-center gap-2 px-4 text-left text-[0.8125rem] font-medium text-ink-2 hover:bg-surface-2 hover:text-ink"
						aria-expanded={showPaused}
						onclick={() => (showPaused = !showPaused)}
					>
						<ChevronRight class={`size-4 shrink-0 transition-transform duration-150 ${showPaused ? 'rotate-90' : ''}`} aria-hidden="true" />
						{m.w9dash_kpi_paused({ count: paused.length })}
					</button>
					{#if showPaused}
						<ul class="m-0 list-none pt-0.5 pr-4 pb-2 pl-10">
							{#each paused as service (service.target_id)}
								<li>
									<a href={withBase(`/targets/${service.target_id}`)} class="flex min-h-10 min-w-0 items-center gap-2 text-[0.8125rem] hover:underline sm:min-h-8">
										<span class="min-w-0 truncate text-ink">{service.name}</span>
										<span class="shrink-0 text-xs text-ink-3">{service.kind}</span>
									</a>
								</li>
							{/each}
						</ul>
					{/if}
				</li>
			{/if}
		</ul>
	</DataState>
</WidgetCard>
