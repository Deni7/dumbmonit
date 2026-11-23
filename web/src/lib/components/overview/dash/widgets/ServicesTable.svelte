<script lang="ts">
	/**
	 * Services: the HTTP, DNS, TLS, SMTP… checks, worst first. Each row: its
	 * state in a plate, the check and its kind, the last 36 checks as beats
	 * (a short bar per success, a full-height one per failure, a hollow stub
	 * where there was no data), the p95 response time and the 30-day
	 * availability. `limit` keeps the first rows. Scrolls sideways on a phone.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { Plate } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { checkSummary, worstFirst, type Span } from '../parts/sites.js';
	import { STATUS_TONE, num, statusWord } from '../parts/labels.js';

	let { span = 5, limit }: { span?: Span; limit?: number } = $props();

	const data = $derived(widgetsStore.data);
	const all = $derived(worstFirst(data?.services ?? []));
	const rows = $derived(limit ? all.slice(0, limit) : all);

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

<WidgetCard {span} flush title={m.w9layout_services_title()} sub={m.w9layout_services_sub()} foot={all.length ? foot : undefined}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={rows.length === 0} emptyText={m.w9layout_services_empty()} class="px-4 pb-4" rows={4}>
		<div class="overflow-x-auto">
			<table class="w-full min-w-[520px] border-collapse text-[0.8125rem]">
				<thead>
					<tr class="border-b border-line text-left text-xs whitespace-nowrap text-ink-3">
						<th scope="col" class="py-1.5 pr-2.5 pl-4 font-medium">{m.w9layout_services_state()}</th>
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_services_check()}</th>
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_services_beats()}</th>
						<th scope="col" class="px-2.5 py-1.5 text-right font-medium">{m.w9layout_services_p95()}</th>
						<th scope="col" class="py-1.5 pr-4 pl-2.5 text-right font-medium">{m.w9layout_services_30d()}</th>
					</tr>
				</thead>
				<tbody>
					{#each rows as service (service.target_id)}
						<tr class="border-b border-line align-middle whitespace-nowrap last:border-b-0">
							<td class="py-2 pr-2.5 pl-4"><Plate tone={STATUS_TONE[service.status]}>{statusWord(service.status)}</Plate></td>
							<td class="max-w-[220px] px-2.5 py-2">
								<a href={withBase(`/targets/${service.target_id}`)} class="group -my-2 block py-2">
									<span class="block truncate font-semibold text-ink group-hover:underline">{service.name}</span>
									<span class="block text-xs text-ink-2">{service.kind}</span>
								</a>
							</td>
							<td class="w-[40%] min-w-[140px] px-2.5 py-2">
								<div class="flex h-[22px] min-w-[110px] items-end gap-[2px]" role="img" aria-label={beatsLabel(service.checks)}>
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
							</td>
							<td class="px-2.5 py-2 text-right font-mono text-ink tabular-nums">{service.p95_ms !== null ? m.w9layout_ms({ value: num(service.p95_ms) }) : '—'}</td>
							<td class="py-2 pr-4 pl-2.5 text-right font-mono text-ink tabular-nums">{availability(service.availability_30d)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</DataState>
</WidgetCard>
