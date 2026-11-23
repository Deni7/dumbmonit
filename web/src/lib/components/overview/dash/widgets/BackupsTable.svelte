<script lang="ts">
	/**
	 * Backups: one row per job, what it backs up where, when it last
	 * succeeded, how long it took, how much changed, its last 14 runs (a filled
	 * square per success, a crossed one per failure, a hollow one where there
	 * was no run) and its state in a plate. `bySite` groups the rows under a
	 * line per site and drops the site column. Scrolls sideways on a phone.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { withBase } from '#lib/paths.js';
	import { formatBinaryBytes, formatDuration } from '#lib/format.js';
	import type { BackupJob } from '#lib/api/index.js';
	import { Plate, type Tone } from '#lib/ui/index.js';
	import WidgetCard from '../WidgetCard.svelte';
	import { widgetsStore } from '../widgets.svelte.js';
	import DataState from '../parts/DataState.svelte';
	import { jobsBySite, jobsToCheck, runCells, type Span } from '../parts/sites.js';
	import { pastWhen } from '../parts/labels.js';

	let { span = 7, bySite = false }: { span?: Span; bySite?: boolean } = $props();

	const data = $derived(widgetsStore.data);
	const jobs = $derived(data?.backups ?? []);
	const groups = $derived(bySite ? jobsBySite(jobs) : [{ site: '', jobs }]);
	const toCheck = $derived(jobsToCheck(jobs));
	const columns = $derived(bySite ? 7 : 8);

	const STATUS: Record<BackupJob['status'], { tone: Tone; label: () => string }> = {
		ok: { tone: 'signal', label: () => m.w9layout_backups_ok() },
		failed: { tone: 'warning', label: () => m.w9layout_backups_failed() },
		running: { tone: 'info', label: () => m.w9layout_backups_running() },
		stale: { tone: 'advisory', label: () => m.w9layout_backups_stale() }
	};

	function runsLabel(cells: (boolean | null)[]): string {
		const ran = cells.filter((c) => c !== null).length;
		const failed = cells.filter((c) => c === false).length;
		return failed ? m.w9layout_backups_runs_failed({ count: ran, failed }) : m.w9layout_backups_runs_ok({ count: ran });
	}

	const sub = $derived(
		jobs.length === 0 ? undefined : toCheck ? m.w9layout_backups_sub_check({ count: jobs.length, check: toCheck }) : m.w9layout_backups_sub_fresh({ count: jobs.length })
	);
</script>

{#snippet runsKey()}
	<span class="hidden items-center gap-3 text-xs text-ink-2 sm:inline-flex" aria-hidden="true">
		<span class="inline-flex items-center gap-1.5"><i class="block size-2.5 rounded-[2px] bg-river-up"></i>{m.w9layout_backups_ok()}</span>
		<span class="inline-flex items-center gap-1.5">
			<svg viewBox="0 0 9 9" class="size-2.5"><rect x="0.5" y="0.5" width="8" height="8" rx="1.5" fill="var(--c-warning-soft)" stroke="var(--c-warning)" /><path d="M2.5 2.5 6.5 6.5M6.5 2.5 2.5 6.5" stroke="var(--c-warning)" stroke-width="1.2" /></svg>
			{m.w9layout_backups_failed()}
		</span>
	</span>
{/snippet}

{#snippet foot()}
	<a href={withBase('/reports')}>{m.w9layout_backups_reports()}</a>
{/snippet}

<WidgetCard {span} flush title={m.w9layout_backups_title()} {sub} right={jobs.length ? runsKey : undefined} foot={jobs.length ? foot : undefined}>
	<DataState loading={widgetsStore.loading && !data} error={data ? null : widgetsStore.error} empty={jobs.length === 0} emptyText={m.w9layout_backups_empty()} class="px-4 pb-4" rows={4}>
		<div class="overflow-x-auto">
			<table class="w-full min-w-[760px] border-collapse text-[0.8125rem] [&>tbody:last-child>tr:last-child]:border-b-0">
				<thead>
					<tr class="border-b border-line text-left text-xs whitespace-nowrap text-ink-3">
						<th scope="col" class="py-1.5 pr-2.5 pl-4 font-medium">{m.w9layout_backups_job()}</th>
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_backups_engine()}</th>
						{#if !bySite}<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_matrix_site()}</th>{/if}
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_backups_last_success()}</th>
						<th scope="col" class="px-2.5 py-1.5 text-right font-medium">{m.w9layout_backups_duration()}</th>
						<th scope="col" class="px-2.5 py-1.5 text-right font-medium">{m.w9layout_backups_changed()}</th>
						<th scope="col" class="px-2.5 py-1.5 font-medium">{m.w9layout_backups_runs()}</th>
						<th scope="col" class="py-1.5 pr-4 pl-2.5 font-medium">{m.w9layout_backups_status()}</th>
					</tr>
				</thead>
				{#each groups as group (group.site)}
					<tbody>
						{#if bySite}
							<tr class="border-b border-line bg-surface-2">
								<th scope="rowgroup" colspan={columns} class="py-1.5 pl-4 text-left text-xs font-semibold text-ink-2">{group.site}</th>
							</tr>
						{/if}
						{#each group.jobs as job (`${job.target_id}:${job.job}`)}
							{@const cells = runCells(job.runs)}
							{@const plate = STATUS[job.status] ?? STATUS.stale}
							<tr class="border-b border-line align-middle whitespace-nowrap">
								<td class="py-2 pr-2.5 pl-4">
									<a href={withBase(`/targets/${job.target_id}`)} class="group -my-2 block py-2">
										<span class="block font-semibold text-ink group-hover:underline">{job.job}</span>
										<span class="block max-w-[260px] truncate text-xs text-ink-2" title={job.detail}>{job.detail}</span>
									</a>
								</td>
								<td class="px-2.5 py-2 text-ink-2">{job.engine}</td>
								{#if !bySite}<td class="px-2.5 py-2 text-ink-2">{job.site}</td>{/if}
								<td class="px-2.5 py-2 text-ink">{job.last_success ? pastWhen(job.last_success) : m.w9layout_backups_never()}</td>
								<td class="px-2.5 py-2 text-right font-mono text-ink-2 tabular-nums">{job.duration_s !== null ? formatDuration(job.duration_s) : '—'}</td>
								<td class="px-2.5 py-2 text-right font-mono text-ink-2 tabular-nums">{formatBinaryBytes(job.changed_bytes)}</td>
								<td class="px-2.5 py-2">
									<svg viewBox="0 0 126 9" width="126" height="9" class="block" role="img" aria-label={runsLabel(cells)}>
										{#each cells as ok, i (i)}
											{#if ok === null}
												<rect x={i * 9 + 0.5} y="1.5" width="6" height="6" rx="1.5" fill="none" stroke="var(--c-line-strong)" />
											{:else if ok}
												<rect x={i * 9} y="1" width="7" height="7" rx="1.5" fill="var(--c-river-up)" />
											{:else}
												<rect x={i * 9} y="1" width="7" height="7" rx="1.5" fill="var(--c-warning-soft)" stroke="var(--c-warning)" />
												<path d={`M${i * 9 + 2} 3L${i * 9 + 5} 6M${i * 9 + 5} 3L${i * 9 + 2} 6`} stroke="var(--c-warning)" stroke-width="1.2" />
											{/if}
										{/each}
									</svg>
								</td>
								<td class="py-2 pr-4 pl-2.5"><Plate tone={plate.tone}>{plate.label()}</Plate></td>
							</tr>
						{/each}
					</tbody>
				{/each}
			</table>
		</div>
	</DataState>
</WidgetCard>
