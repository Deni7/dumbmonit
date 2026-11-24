<script lang="ts">
	/**
	 * Status › SLOs: one row per service level objective — its target and
	 * window, the availability attained, what is left of the error budget (a
	 * bar with its words, never colour alone) and the current burn rate. A row
	 * opens in place: the budget over the window, the burn rate by window and,
	 * for those who may write, edit and delete. Everyone reads; administrators write,
	 * and so do operators restricted by their teams, on their own monitors.
	 * `?slo=<id>` opens that row (the link of a burn-rate alert).
	 */
	import { Gauge, Plus } from 'lucide-svelte';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { deleteSlo, getSlo, listSlos, type Slo, type SloDetail } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { budgetFill, budgetTone, formatAvailability, formatRate, shownRate, verdictTone } from '#lib/slo.js';
	import { Button, Confirm, EmptyState, ErrorNotice, Panel, Plate, Skeleton, type Tone } from '#lib/ui/index.js';
	import Chart from '#lib/components/Chart.svelte';
	import SloForm from './SloForm.svelte';

	/** Administrators, and team-restricted operators (the server checks each monitor). */
	const canWrite = $derived(auth.isAdmin || (auth.scoped && auth.canOperate));

	let slos = $state<Slo[]>([]);
	let loading = $state(true);
	let loadError = $state<unknown>(null);

	async function load(signal?: AbortSignal) {
		loadError = null;
		try {
			slos = await listSlos(signal);
			const asked = Number(page.url.searchParams.get('slo'));
			const linked = slos.find((s) => s.id === asked);
			if (linked && open === null) void toggle(linked);
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			loadError = cause;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	// --- Form ------------------------------------------------------------------

	/** `'new'`, the id being edited, or nothing. */
	let editing = $state<'new' | number | null>(null);

	function saved(next: SloDetail) {
		const exists = slos.some((s) => s.id === next.id);
		slos = (exists ? slos.map((s) => (s.id === next.id ? next : s)) : [...slos, next]).sort((a, b) => a.name.localeCompare(b.name));
		editing = null;
		open = next.id;
		detail = next;
	}

	// --- Detail ----------------------------------------------------------------

	let open = $state<number | null>(null);
	let detail = $state<SloDetail | null>(null);
	let detailError = $state<unknown>(null);
	let detailRun = 0;

	async function toggle(slo: Slo) {
		if (open === slo.id) {
			open = null;
			return;
		}
		open = slo.id;
		detail = null;
		detailError = null;
		const run = ++detailRun;
		try {
			const loaded = await getSlo(slo.id);
			if (run === detailRun) detail = loaded;
		} catch (cause) {
			if (run === detailRun) detailError = cause;
		}
	}

	let deleting = $state<number | null>(null);
	let deleteError = $state<unknown>(null);

	async function remove(slo: Slo) {
		deleting = slo.id;
		deleteError = null;
		try {
			await deleteSlo(slo.id);
			slos = slos.filter((s) => s.id !== slo.id);
			open = null;
		} catch (cause) {
			deleteError = cause;
		} finally {
			deleting = null;
		}
	}

	// --- Words -------------------------------------------------------------------

	const FILL: Record<string, string> = { signal: 'bg-signal', advisory: 'bg-advisory', warning: 'bg-warning', ghost: 'bg-ghost' };

	function budgetWords(slo: Slo): string {
		const budget = slo.status.budget;
		if (!slo.status.measured) return m.slo_not_measured();
		if (!budget) return m.slo_no_data();
		if (budget.remaining_percent <= 0) return m.slo_budget_spent();
		return m.slo_budget_left({ percent: Math.round(budget.remaining_percent) });
	}

	function burn(slo: Slo): { tone: Tone; label: string } {
		const rate = formatRate(shownRate(slo.status.burn, slo.status.verdict));
		if (!slo.status.measured || rate === null) return { tone: 'ghost', label: slo.status.measured ? m.slo_no_data() : m.slo_not_measured() };
		const tone = verdictTone(slo.status.verdict);
		if (slo.status.verdict === 'fast') return { tone, label: m.slo_burn_fast({ rate }) };
		if (slo.status.verdict === 'slow') return { tone, label: m.slo_burn_slow({ rate }) };
		return { tone, label: m.slo_burn_normal({ rate }) };
	}

	const minutes = (value: number) => Math.round(value * 10) / 10;

	const RATE_WINDOWS = $derived([
		{ key: 'm5' as const, label: m.slo_window_5m() },
		{ key: 'm30' as const, label: m.slo_window_30m() },
		{ key: 'h1' as const, label: m.slo_window_1h() },
		{ key: 'h6' as const, label: m.slo_window_6h() }
	]);
</script>

<div class="grid gap-4">
	<div class="flex flex-wrap items-start justify-between gap-3">
		<p class="max-w-2xl text-sm text-ink-2 max-sm:hidden">{m.slo_tab_description()}</p>
		{#if canWrite && editing === null}
			<Button variant="secondary" onclick={() => (editing = 'new')}>
				<Plus class="size-4" aria-hidden="true" />
				{m.slo_new()}
			</Button>
		{/if}
	</div>

	{#if editing === 'new'}
		<SloForm slo={null} onsaved={saved} oncancel={() => (editing = null)} />
	{/if}

	{#if loadError}
		<ErrorNotice error={loadError} title={m.slo_load_error()} onretry={() => void load()} />
	{:else if loading}
		<div class="grid gap-2">
			<Skeleton class="h-16 w-full" />
			<Skeleton class="h-16 w-full" />
		</div>
	{:else if slos.length === 0}
		{#if editing !== 'new'}
			<EmptyState
				icon={Gauge}
				title={m.slo_empty_title()}
				description={m.slo_empty_description()}
				primary={canWrite ? { label: m.slo_new(), onclick: () => (editing = 'new') } : undefined}
			/>
		{/if}
	{:else}
		<Panel padded={false}>
			<ul class="divide-y divide-line" role="list" aria-label={m.slo_list_aria()}>
				{#each slos as slo (slo.id)}
					{@const tone = budgetTone(slo.status)}
					{@const rate = burn(slo)}
					<li>
						{#if editing === slo.id}
							<div class="p-3">
								<SloForm {slo} onsaved={saved} oncancel={() => (editing = null)} />
							</div>
						{:else}
							<button
								type="button"
								class="grid min-h-16 w-full gap-x-6 gap-y-2 px-4 py-3 text-left hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-signal sm:grid-cols-[minmax(0,1.4fr)_6.5rem_minmax(0,1fr)_minmax(0,auto)] sm:items-center sm:px-5"
								aria-expanded={open === slo.id}
								aria-controls={`slo-${slo.id}-detail`}
								onclick={() => toggle(slo)}
							>
								<span class="min-w-0">
									<span class="flex flex-wrap items-center gap-2">
										<span class="truncate font-semibold text-ink">{slo.name}</span>
										{#if !slo.enabled}<Plate tone="ghost" label={m.slo_paused()} bare />{/if}
									</span>
									<span class="mt-0.5 block text-[0.8125rem] text-ink-2">{m.slo_target_window({ target: slo.objective, days: slo.window_days })}</span>
								</span>
								<span class="tnum text-sm text-ink">
									<span class="sr-only">{m.slo_attained_label()}: </span>
									{slo.status.attained === null ? '–' : formatAvailability(slo.status.attained, slo.objective)}
								</span>
								<span class="grid min-w-0 gap-1">
									<span class="h-2 overflow-hidden rounded-full bg-canvas-deep" aria-hidden="true">
										<span class={`block h-full rounded-full ${FILL[tone]}`} style={`width: ${budgetFill(slo.status.budget?.remaining_percent)}%`}></span>
									</span>
									<span class="text-[0.8125rem] text-ink-2"><span class="sr-only">{m.slo_budget_label()}: </span>{budgetWords(slo)}</span>
								</span>
								<span class="justify-self-start sm:justify-self-end"><Plate tone={rate.tone} label={rate.label} /></span>
							</button>

							{#if open === slo.id}
								<div id={`slo-${slo.id}-detail`} class="grid gap-4 border-t border-line bg-canvas-deep/40 px-4 py-4 sm:px-5">
									{#if detailError}
										<ErrorNotice error={detailError} title={m.slo_load_error()} />
									{:else if !detail || detail.id !== slo.id}
										<Skeleton class="h-48 w-full" />
									{:else}
										{#if slo.description}<p class="max-w-2xl text-sm text-ink">{slo.description}</p>{/if}
										<div class="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
											<div class="min-w-0">
												<p class="mb-2 text-sm font-semibold text-ink">{m.slo_detail_chart_title()}</p>
												{#if detail.burndown.length > 1}
													<Chart
														series={[{ label: m.slo_detail_chart_series(), points: detail.burndown }]}
														unit="%"
														height={200}
														tone={tone === 'ghost' ? 'signal' : tone}
														thresholds={[{ value: 0, tone: 'warning', label: m.slo_detail_chart_zero() }]}
													/>
												{:else}
													<p class="text-sm text-ink-2">{m.slo_detail_chart_empty()}</p>
												{/if}
											</div>
											<div class="grid content-start gap-3">
												{#if detail.status.budget}
													<p class="text-sm text-ink">
														{m.slo_detail_minutes({ used: minutes(detail.status.budget.consumed_minutes), total: minutes(detail.status.budget.total_minutes) })}
													</p>
												{/if}
												<div>
													<p class="text-sm font-semibold text-ink">{m.slo_detail_rates_title()}</p>
													<dl class="mt-1.5 grid grid-cols-4 gap-2">
														{#each RATE_WINDOWS as item (item.key)}
															<div class="rounded-lg border border-line bg-surface px-2 py-1.5">
																<dt class="text-[0.75rem] text-ink-2">{item.label}</dt>
																<dd class="tnum text-sm font-semibold text-ink">{formatRate(detail.status.burn[item.key]) ?? '–'}</dd>
															</div>
														{/each}
													</dl>
													<p class="mt-2 text-[0.8125rem] text-ink-2">{m.slo_detail_rates_help()}</p>
												</div>
											</div>
										</div>
										{#if canWrite}
											<div class="flex flex-wrap items-center gap-2">
												<Button variant="secondary" size="sm" onclick={() => (editing = slo.id)}>{m.slo_edit()}</Button>
												<Confirm confirmLabel={m.slo_delete_confirm()} loading={deleting === slo.id} onconfirm={() => remove(slo)}>{m.slo_delete()}</Confirm>
											</div>
											{#if deleteError}<ErrorNotice error={deleteError} title={m.slo_delete_error()} />{/if}
										{/if}
									{/if}
								</div>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
		</Panel>
	{/if}
</div>
