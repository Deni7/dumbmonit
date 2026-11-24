<script lang="ts">
	/**
	 * Alerts › Noise: which rules talk too much, which alerts keep coming
	 * back, and how long it takes to react — over the last 7, 30 or 90 days of
	 * history (`GET /api/alerts/noise`). For admins and operators; a
	 * team-restricted account sees its own devices and teams only (the server
	 * filters).
	 *
	 * Each figure is written once: the overall line, then one row per rule
	 * and per team. A noisy rule links to its editor and, when it can be
	 * replayed, straight to "Test on history" (admins only: the editor is
	 * theirs).
	 */
	import { onDestroy, untrack } from 'svelte';
	import type { AlertNoise, AlertNoiseStats, NoiseDays } from '#lib/api/index.js';
	import { NOISE_DAYS, getAlertNoise } from '#lib/api/index.js';
	import { EmptyState, ErrorNotice, Panel, Skeleton } from '#lib/ui/index.js';
	import Segmented from '#lib/components/devices/Segmented.svelte';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { withBase } from '#lib/paths.js';
	import { formatDuration } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';

	let days = $state<NoiseDays>(30);
	let report = $state<AlertNoise | null>(null);
	let loading = $state(true);
	let error = $state<unknown>(null);
	let controller: AbortController | null = null;
	onDestroy(() => controller?.abort());

	const dayOptions = $derived(NOISE_DAYS.map((n) => ({ id: String(n) as `${NoiseDays}`, label: m.w5alerting_noise_window({ count: n }) })));

	async function load(span: NoiseDays) {
		controller?.abort();
		controller = new AbortController();
		const signal = controller.signal;
		loading = true;
		error = null;
		try {
			report = await getAlertNoise(span, signal);
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		} finally {
			if (!signal.aborted) loading = false;
		}
	}

	$effect(() => {
		const span = days;
		untrack(() => void load(span));
	});

	const maxNotifications = $derived(Math.max(1, ...(report?.rules.map((rule) => rule.notifications) ?? [0])));

	/** "4 min", or a dash when nothing was acknowledged / resolved. */
	function delay(secs: number | null): string {
		return secs === null ? m.w5alerting_noise_no_delay() : formatDuration(secs);
	}

	function ruleLink(uid: string, extra: 'edit' | 'backtest'): string {
		return withBase(`/alerts?tab=rules&rule=${encodeURIComponent(uid)}&${extra}=1`);
	}

	const linkClass = 'inline-flex min-h-10 items-center font-semibold text-ink-2 underline decoration-line-strong underline-offset-2 hover:text-ink';
</script>

{#snippet delays(stats: AlertNoiseStats)}
	<span>{m.w5alerting_noise_mtta({ delay: delay(stats.mtta_secs) })}</span>
	<span>{m.w5alerting_noise_mttr({ delay: delay(stats.mttr_secs) })}</span>
{/snippet}

<div class="grid gap-5">
	<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
		<p class="max-w-prose text-sm text-ink-2">{m.w5alerting_noise_intro()}</p>
		<Segmented
			options={dayOptions}
			value={String(days) as `${NoiseDays}`}
			onchange={(v) => (days = Number(v) as NoiseDays)}
			label={m.w5alerting_noise_window_label()}
			size="sm"
		/>
	</div>

	{#if error}
		<ErrorNotice {error} title={m.w5alerting_noise_error()} onretry={() => void load(days)} />
	{:else if loading && !report}
		<div class="grid gap-2.5">
			{#each { length: 3 } as _, i (i)}
				<Skeleton class="h-16 w-full" />
			{/each}
		</div>
	{:else if report && report.overall.fired === 0}
		<EmptyState title={m.w5alerting_noise_empty({ count: report.days })} description={m.w5alerting_noise_empty_help()} tone="signal" />
	{:else if report}
		<dl class="grid gap-3 sm:grid-cols-4" aria-busy={loading}>
			<div class="rounded-[var(--radius-plate)] border border-line bg-surface px-4 py-3">
				<dt class="text-[0.8125rem] text-ink-2">{m.w5alerting_noise_fired_label()}</dt>
				<dd class="tnum text-xl font-semibold text-ink">{report.overall.fired}</dd>
			</div>
			<div class="rounded-[var(--radius-plate)] border border-line bg-surface px-4 py-3">
				<dt class="text-[0.8125rem] text-ink-2">{m.w5alerting_noise_notifications_label()}</dt>
				<dd class="tnum text-xl font-semibold text-ink">{report.overall.notifications}</dd>
			</div>
			<div class="rounded-[var(--radius-plate)] border border-line bg-surface px-4 py-3">
				<dt class="text-[0.8125rem] text-ink-2">{m.w5alerting_noise_mtta_label()}</dt>
				<dd class="tnum text-xl font-semibold text-ink">{delay(report.overall.mtta_secs)}</dd>
				<dd class="text-[0.75rem] text-ink-2">{m.w5alerting_noise_acknowledged({ count: report.overall.acknowledged })}</dd>
			</div>
			<div class="rounded-[var(--radius-plate)] border border-line bg-surface px-4 py-3">
				<dt class="text-[0.8125rem] text-ink-2">{m.w5alerting_noise_mttr_label()}</dt>
				<dd class="tnum text-xl font-semibold text-ink">{delay(report.overall.mttr_secs)}</dd>
				<dd class="text-[0.75rem] text-ink-2">{m.w5alerting_noise_resolved({ count: report.overall.resolved })}</dd>
			</div>
		</dl>
		{#if report.truncated}
			<p class="text-[0.8125rem] text-ink-2">{m.w5alerting_noise_truncated()}</p>
		{/if}

		<Panel title={m.w5alerting_noise_rules_title()} description={m.w5alerting_noise_rules_help()}>
			<ol class="grid gap-3">
				{#each report.rules as rule (rule.rule_uid)}
					<li class="grid gap-1.5 border-b border-line pb-3 last:border-b-0 last:pb-0">
						<div class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
							<p class="min-w-0 truncate text-sm font-semibold text-ink" title={rule.rule_name}>{rule.rule_name}</p>
							<p class="tnum flex flex-wrap gap-x-3 text-sm text-ink">
								<span>{m.w5alerting_noise_row_notifications({ count: rule.notifications })}</span>
								<span class="text-ink-2">{m.w5alerting_noise_row_fired({ count: rule.fired })}</span>
							</p>
						</div>
						<!-- Share of notifications, against the noisiest rule. The figure is written just above. -->
						<div class="h-1.5 w-full overflow-hidden rounded-full bg-canvas-deep" aria-hidden="true">
							<div class="h-full rounded-full bg-warning" style:width={`${(rule.notifications / maxNotifications) * 100}%`}></div>
						</div>
						<div class="flex flex-wrap items-center gap-x-4 text-[0.8125rem] text-ink-2 tnum">
							{@render delays(rule)}
							{#if auth.isAdmin && rule.rule_id !== null}
								<a href={ruleLink(rule.rule_uid, 'edit')} class={linkClass}>{m.w5alerting_noise_edit_rule()}</a>
								{#if rule.replayable}
									<a href={ruleLink(rule.rule_uid, 'backtest')} class={linkClass}>{m.w5alerting_noise_test_rule()}</a>
								{/if}
							{/if}
						</div>
					</li>
				{/each}
			</ol>
		</Panel>

		{#if report.flapping.length > 0}
			<Panel title={m.w5alerting_noise_flapping_title()} description={m.w5alerting_noise_flapping_help()}>
				<ul class="grid gap-2">
					{#each report.flapping as flap (flap.fingerprint)}
						<li class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-0.5 text-sm">
							<span class="min-w-0 text-ink">
								{flap.target_name
									? m.w5alerting_noise_flap_on({ rule: flap.rule_name, device: flap.target_name })
									: flap.rule_name}
							</span>
							<span class="tnum text-ink-2">{m.w5alerting_noise_flap_changes({ count: flap.changes })}</span>
						</li>
					{/each}
				</ul>
			</Panel>
		{/if}

		{#if report.teams.length > 0}
			<Panel title={m.w5alerting_noise_teams_title()}>
				<ul class="grid gap-3">
					{#each report.teams as team (team.team_id)}
						<li class="grid gap-1 border-b border-line pb-3 last:border-b-0 last:pb-0">
							<div class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
								<p class="min-w-0 truncate text-sm font-semibold text-ink" title={team.team_name}>{team.team_name}</p>
								<p class="tnum flex flex-wrap gap-x-3 text-sm text-ink">
									<span>{m.w5alerting_noise_row_notifications({ count: team.notifications })}</span>
									<span class="text-ink-2">{m.w5alerting_noise_row_fired({ count: team.fired })}</span>
								</p>
							</div>
							<div class="flex flex-wrap gap-x-4 text-[0.8125rem] text-ink-2 tnum">
								{@render delays(team)}
							</div>
						</li>
					{/each}
				</ul>
			</Panel>
		{/if}
	{/if}
</div>
