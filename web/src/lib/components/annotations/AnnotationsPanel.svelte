<script lang="ts">
	/**
	 * Change markers (deployments, updates, maintenance) for a chart area: a
	 * switch that shows or hides them on the charts, the list of those in the
	 * visible period (also the text alternative of the chart markers: each one
	 * reads out, with its time, text and link), and, on a monitor page, a form
	 * to add one by hand. The page passes `events` to its charts.
	 *
	 * Give `seconds` for a window that rolls up to now (monitor page), or `from`
	 * and `to` (seconds) for a fixed one (the explorer, after a run).
	 */
	import { Flag, Plus } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { deleteAnnotation, listAnnotations, type Annotation } from '#lib/api/index.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { startPolling } from '#lib/poll.js';
	import { formatDateTime } from '#lib/format.js';
	import { toChartEvents } from '#lib/annotations/events.js';
	import { snapRange } from '#lib/annotations/layout.js';
	import type { ChartEvent } from '#lib/components/Chart.svelte';
	import { Button, Confirm, ErrorNotice, Toggle } from '#lib/ui/index.js';
	import AnnotationForm from './AnnotationForm.svelte';

	interface Props {
		/** Monitor whose markers to show (and to add to); none: every marker. */
		targetId?: number;
		/** A window rolling up to now. */
		seconds?: number;
		/** Or a fixed window, in seconds since the epoch. */
		from?: number;
		to?: number;
		/** What the charts draw: empty while the switch is off. */
		events?: ChartEvent[];
	}

	let { targetId, seconds, from, to, events = $bindable([]) }: Props = $props();

	const STORAGE_KEY = 'dumbmonit.annotations.shown';
	/** Markers listed at once; the charts still draw them all. */
	const LISTED = 6;

	function readShown(): boolean {
		try {
			return localStorage.getItem(STORAGE_KEY) !== 'off';
		} catch {
			return true;
		}
	}

	let shown = $state(readShown());
	let annotations = $state.raw<Annotation[]>([]);
	let loadError = $state<unknown>(null);
	let adding = $state(false);

	const canWrite = $derived(auth.canOperate && targetId !== undefined);

	function setShown(value: boolean) {
		try {
			localStorage.setItem(STORAGE_KEY, value ? 'on' : 'off');
		} catch {
			// Private mode: the switch still works for this visit.
		}
	}

	$effect(() => {
		events = shown ? toChartEvents(annotations) : [];
	});

	async function load(signal?: AbortSignal) {
		const now = Math.floor(Date.now() / 1000);
		const span =
			seconds !== undefined
				? snapRange(now - seconds, now)
				: from !== undefined && to !== undefined
					? snapRange(from, to)
					: null;
		if (!span) {
			annotations = [];
			return;
		}
		try {
			const found = await listAnnotations({
				from: new Date(span.from * 1000),
				to: new Date(span.to * 1000),
				target: targetId,
				signal
			});
			annotations = Array.isArray(found) ? found : [];
			loadError = null;
		} catch (cause) {
			if (signal?.aborted) return;
			loadError = cause;
			return false;
		}
	}

	$effect(() => {
		void targetId;
		void seconds;
		void from;
		void to;
		return startPolling(load, 60_000);
	});

	/** Most recent first, as many as the list holds. */
	const listed = $derived([...annotations].reverse().slice(0, LISTED));

	function created(next: Annotation) {
		annotations = [...annotations, next].sort((a, b) => a.at.localeCompare(b.at));
		adding = false;
	}

	async function remove(item: Annotation) {
		try {
			await deleteAnnotation(item.id);
			annotations = annotations.filter((a) => a.id !== item.id);
			loadError = null;
		} catch (cause) {
			loadError = cause;
		}
	}

	const SOURCE_LABEL: Record<string, () => string> = {
		api: () => m.w5annot_source_api(),
		ci: () => m.w5annot_source_ci(),
		ui: () => m.w5annot_source_ui()
	};

	function scopeLabel(item: Annotation): string {
		switch (item.scope.kind) {
			case 'global':
				return m.w5annot_scope_global();
			case 'monitors':
				return m.w5annot_scope_monitors();
			case 'folder':
				return m.w5annot_scope_folder({ folder: item.scope.folder });
			default:
				return m.w5annot_scope_tags({ tags: item.scope.tags.join(', ') });
		}
	}

	function metaOf(item: Annotation): string {
		const source = SOURCE_LABEL[item.source]?.() ?? item.source;
		return item.author
			? m.w5annot_list_meta({ source, author: item.author, scope: scopeLabel(item) })
			: m.w5annot_list_meta_anon({ source, scope: scopeLabel(item) });
	}

	function whenOf(item: Annotation): string {
		return item.until
			? m.w5annot_list_window({ start: formatDateTime(item.at), end: formatDateTime(item.until) })
			: formatDateTime(item.at);
	}
</script>

<section class="mb-4 rounded-lg border border-line bg-surface px-4 py-3" aria-labelledby="annotations-heading">
	<div class="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
		<h3 id="annotations-heading" class="flex items-center gap-2 text-sm font-semibold text-ink">
			<Flag class="size-4 text-ink-2" aria-hidden="true" />
			{m.w5annot_panel_title()}
		</h3>
		<div class="flex flex-wrap items-center gap-3">
			<label class="inline-flex min-h-10 items-center gap-2 text-[0.8125rem] text-ink-2">
				<Toggle bind:checked={shown} onchange={setShown} label={m.w5annot_panel_toggle()} />
				<span aria-hidden="true">{m.w5annot_panel_toggle_short()}</span>
			</label>
			{#if canWrite && !adding}
				<Button size="sm" onclick={() => (adding = true)}><Plus class="size-4" aria-hidden="true" />{m.w5annot_panel_add()}</Button>
			{/if}
		</div>
	</div>

	{#if adding && targetId !== undefined}
		<div class="mt-3">
			<AnnotationForm {targetId} oncreated={created} oncancel={() => (adding = false)} />
		</div>
	{/if}

	{#if loadError}
		<div class="mt-3"><ErrorNotice error={loadError} onretry={() => void load()} /></div>
	{/if}

	{#if listed.length === 0}
		{#if !loadError}<p class="mt-2 text-[0.8125rem] text-ink-2">{m.w5annot_panel_empty()}</p>{/if}
	{:else}
		<ul class="mt-2 divide-y divide-line" aria-label={m.w5annot_panel_list_label()}>
			{#each listed as item (item.id)}
				<li class="flex min-w-0 flex-wrap items-start justify-between gap-x-4 gap-y-1 py-2 text-[0.8125rem]">
					<div class="min-w-0 flex-1">
						<p class="break-words font-medium text-ink">{item.title}</p>
						<p class="tnum text-ink-2">{whenOf(item)}</p>
						{#if item.text}<p class="break-words text-ink-2">{item.text}</p>{/if}
						<p class="text-ink-3">{metaOf(item)}</p>
						{#if /^https?:\/\//.test(item.link)}
							<a
								href={item.link}
								target="_blank"
								rel="noopener noreferrer"
								class="inline-flex min-h-10 items-center font-medium text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink"
								>{m.w5annot_list_link()}</a
							>
						{/if}
					</div>
					{#if auth.canOperate}
						<Confirm confirmLabel={m.w5annot_list_delete_confirm()} onconfirm={() => remove(item)} class="max-sm:h-10">
							{m.w5annot_list_delete()}<span class="sr-only"> {item.title}</span>
						</Confirm>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>
