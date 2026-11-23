<script lang="ts">
	/** Instruments of a service monitor: availability, response time, uptime history, HTTP phases and scenario steps. */
	import { m } from '#lib/paraglide/messages.js';
	import type { Target } from '#lib/api/index.js';
	import { formatLatency, formatPercent } from '#lib/format.js';
	import { HISTORY_SLOTS, PHASE_TOKENS, type HistorySlot, type UptimeSummary } from '#lib/metrics.js';
	import type { thresholdsFor } from '#lib/chart-context.js';
	import { lastRun } from '#lib/components/device-form/scenario-steps.js';
	import ScenarioSteps from '#lib/components/devices/ScenarioSteps.svelte';
	import Chart, { type ChartEvent, type Serie } from '#lib/components/Chart.svelte';
	import Readout from '#lib/components/Readout.svelte';
	import Figure from '#lib/components/devices/Figure.svelte';
	import PhaseBreakdown from '#lib/components/devices/PhaseBreakdown.svelte';
	import UptimeBar from '#lib/components/devices/UptimeBar.svelte';
	import { Button, EmptyState, Panel } from '#lib/ui/index.js';

	interface Props {
		target: Target;
		summary: UptimeSummary | null;
		availability: number | null;
		availabilityWindow: string;
		rangeLabel: string;
		checks: number | null;
		history: HistorySlot[];
		responseTimes: Serie[];
		phaseTimes: Serie[];
		scenarioRuns: ReturnType<typeof lastRun>;
		syncKey: string;
		chartEvents: ChartEvent[];
		thresholdsOf: (metric: string, unit: string, scale?: number) => ReturnType<typeof thresholdsFor>;
		previousResponse: (window: { start: number; end: number }) => Promise<Serie[]>;
		monitorOn: boolean;
		probing: boolean;
		probe: () => void | Promise<void>;
	}

	let {
		target,
		summary,
		availability,
		availabilityWindow,
		rangeLabel,
		checks,
		history,
		responseTimes,
		phaseTimes,
		scenarioRuns,
		syncKey,
		chartEvents,
		thresholdsOf,
		previousResponse,
		monitorOn,
		probing,
		probe
	}: Props = $props();

	function availabilityTone(value: number | null): 'ink' | 'signal' | 'advisory' | 'warning' {
		if (value === null) return 'ink';
		if (value >= 99) return 'signal';
		if (value >= 95) return 'advisory';
		return 'warning';
	}
</script>

	<div class="graticule grid grid-cols-1 gap-4 pb-1 sm:grid-cols-3">
		<Figure
			label={m.devices_target_availability()}
			value={availability === null ? null : formatPercent(availability)}
			tone={availabilityTone(availability)}
			hint={availabilityWindow}
		/>
		<Figure
			label={m.devices_target_response_time()}
			value={summary?.responseSeconds == null ? null : formatLatency(summary.responseSeconds)}
			hint={m.devices_target_avg_hour()}
		/>
		<Readout label={m.devices_target_checks()} value={checks} hint={m.devices_target_last_range({ range: rangeLabel })} />
	</div>

	<Panel title={m.devices_target_history()} description={m.devices_target_history_desc({ slots: HISTORY_SLOTS, range: rangeLabel })} class="mt-6">
		<UptimeBar slots={history} reason={summary?.status?.reason ?? null} />
	</Panel>

	<Panel title={m.devices_target_response_time()} class="mt-4">
		{#snippet aside()}<span class="label-tape label-tape-unit">ms</span>{/snippet}
		{#if responseTimes.length > 0}
			<Chart series={responseTimes} unit="ms" height={220} {syncKey} events={chartEvents} thresholds={thresholdsOf('dumbmonit_probe_duration_seconds', 'ms', 1000)} loadPrevious={previousResponse} />
		{:else}
			<EmptyState title={m.devices_target_no_check_title()} description={target.enabled ? m.devices_target_no_check_desc() : m.devices_target_disabled_empty()}>
				{#snippet action()}
					{#if monitorOn}
						<Button variant="secondary" size="sm" onclick={probe} loading={probing}>{m.devices_target_probe_now()}</Button>
					{/if}
				{/snippet}
			</EmptyState>
		{/if}
	</Panel>

	{#if phaseTimes.length > 0}
		<Panel title={m.monitor_phases_title()} description={m.monitor_phases_desc()} class="mt-4">
			{#snippet aside()}<span class="label-tape label-tape-unit">ms</span>{/snippet}
			<Chart series={phaseTimes} unit="ms" height={220} stacked colorTokens={[...PHASE_TOKENS]} {syncKey} events={chartEvents} />
			<div class="mt-4 border-t border-line pt-4">
				<PhaseBreakdown series={phaseTimes} />
			</div>
		</Panel>
	{/if}

	{#if scenarioRuns.length > 0}
		<Panel title={m.monitor_steps_title()} description={m.monitor_steps_desc()} class="mt-4">
			<ScenarioSteps runs={scenarioRuns} reason={summary?.status?.reason ?? null} />
		</Panel>
	{/if}
