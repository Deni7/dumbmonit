<script lang="ts">
	/** Outcome of "Test connection" in the device form, announced politely. */
	import { CheckCircle2, XCircle } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { TargetTestReport } from '#lib/api/index.js';
	import { ErrorNotice } from '#lib/ui/index.js';
	import { identitySummary } from './test-connection';

	let { report, error }: { report: TargetTestReport | null; error: unknown } = $props();
</script>

<div aria-live="polite" class="grid gap-1">
	{#if report?.ok}
		{@const identity = identitySummary(report.sysname, report.sysdescr)}
		<p class="flex items-start gap-2 text-sm font-medium text-ink">
			<CheckCircle2 class="mt-0.5 size-4 shrink-0 text-signal-ink" aria-hidden="true" />
			<span>
				{identity ? m.deviceform_test_ok_identity({ identity }) : m.deviceform_test_ok()}
			</span>
		</p>
		{#if report.profile_id}
			<p class="pl-6 text-[0.8125rem] text-ink-2">{m.deviceform_test_profile({ profile: report.profile_id })}</p>
		{/if}
	{:else if report}
		<p class="flex items-start gap-2 text-sm font-medium text-warning-ink">
			<XCircle class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
			<span>{m.deviceform_test_failed({ error: report.error ?? '' })}</span>
		</p>
		<p class="pl-6 text-[0.8125rem] text-ink-2">
			{report.unreachable ? m.deviceform_test_hint_down() : m.deviceform_test_hint_config()}
		</p>
	{:else if error}
		<ErrorNotice {error} title={m.deviceform_test_error()} />
	{/if}
</div>
