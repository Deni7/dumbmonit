<script lang="ts">
	/**
	 * The last run of an HTTP scenario, step by step: its name (or "METHOD
	 * url"), the HTTP status it got, how long it took and how it ended. The
	 * result is a mark and a word, never colour alone; the failed step also
	 * says why, from the probe's failure reason.
	 */
	import { formatFailureReason, formatLatency } from '#lib/format.js';
	import { m } from '#lib/paraglide/messages.js';
	import { StatusMark } from '#lib/ui/index.js';
	import type { StepResult, StepRun } from '#lib/components/device-form/scenario-steps.js';

	interface Props {
		runs: StepRun[];
		/** Failure reason of the last check (`status`, `extract`…), for the failed step. */
		reason: string | null;
	}

	let { runs, reason }: Props = $props();

	const MARK: Record<StepResult, { glyph: 'none' | 'cross' | 'ring'; tone: 'signal' | 'warning' | 'ghost' }> = {
		passed: { glyph: 'none', tone: 'signal' },
		failed: { glyph: 'cross', tone: 'warning' },
		skipped: { glyph: 'ring', tone: 'ghost' }
	};

	function resultLabel(result: StepResult): string {
		if (result === 'passed') return m.monitor_steps_passed();
		if (result === 'failed') return m.monitor_steps_failed();
		return m.monitor_steps_skipped();
	}
</script>

<ol class="divide-y divide-line">
	{#each runs as run (run.index)}
		{@const mark = MARK[run.result]}
		<li class="grid grid-cols-[1.5rem_minmax(0,1fr)_auto] items-start gap-x-3 gap-y-0.5 py-2.5">
			<span class="tnum pt-px text-right text-[0.8125rem] font-semibold text-ink-3" aria-hidden="true">{run.index}</span>
			<div class="min-w-0">
				<p class={`truncate text-sm font-semibold ${run.result === 'skipped' ? 'text-ink-3' : 'text-ink'}`} title={run.title || undefined}>
					{run.title || m.monitor_steps_untitled({ number: String(run.index) })}
				</p>
				<p class="flex flex-wrap items-center gap-x-2 text-[0.8125rem] text-ink-2">
					<StatusMark glyph={mark.glyph} tone={mark.tone} label={resultLabel(run.result)} />
					<span>{resultLabel(run.result)}</span>
					{#if run.result === 'failed' && reason}<span class="text-warning-ink">{formatFailureReason(reason)}</span>{/if}
				</p>
			</div>
			<p class="tnum text-right text-[0.8125rem] text-ink-2">
				{#if run.status !== null}<span class="font-semibold text-ink">{run.status}</span>{/if}
				{#if run.seconds !== null}<span class="block">{formatLatency(run.seconds)}</span>{/if}
			</p>
		</li>
	{/each}
</ol>
