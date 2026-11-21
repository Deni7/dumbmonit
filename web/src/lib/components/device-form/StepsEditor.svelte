<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	/**
	 * Step editor of an HTTP scenario (`input: steps`): an ordered list of
	 * requests, each one folded to its number, name and "METHOD url" line and
	 * opened to edit its method, URL, headers, body, expected status, checks and
	 * extractions. The tag holds the JSON array the server reads; lines being
	 * typed (a header with no name yet) live here, as for `rows`.
	 *
	 * What the server would refuse anyway (a step without URL, a variable used
	 * before any step extracts it) is said under the step, folded or not.
	 */
	import { untrack } from 'svelte';
	import { ArrowDown, ArrowUp, ChevronDown, Plus, Trash2, X } from 'lucide-svelte';
	import type { CollectorOption } from '#lib/api/index.js';
	import { Button, Field, StatusMark } from '#lib/ui/index.js';
	import {
		CHECK_TYPES,
		MAX_STEPS,
		METHODS,
		SOURCES,
		emptyCheck,
		emptyExtraction,
		emptyStep,
		moveStep,
		parseSteps,
		serializeSteps,
		stepIssues,
		type CheckType,
		type ScenarioStep,
		type Source,
		type StepIssue
	} from './scenario-steps';

	interface Props {
		option: CollectorOption;
		/** The tag as saved: a JSON array of steps, or nothing yet. */
		value: string | undefined;
		error?: string;
		onchange: (value: string) => void;
	}

	let { option, value, error, onchange }: Props = $props();

	interface Item {
		id: number;
		open: boolean;
		step: ScenarioStep;
	}

	let nextId = 0;
	// Seeded once, like the rest of the form: a single step opens, a longer
	// scenario shows its outline first.
	let items = $state<Item[]>(
		untrack(() => {
			const parsed = parseSteps(value);
			return parsed.map((step) => ({ id: nextId++, open: parsed.length === 1, step }));
		})
	);

	const id = $derived(`option-${option.key}`);
	const issues = $derived(stepIssues(items.map((item) => item.step)));

	function commit() {
		onchange(serializeSteps(items.map((item) => item.step)));
	}

	/** Applies a change to the steps, then stores the tag. */
	function edit(change: () => void) {
		change();
		commit();
	}

	function add() {
		if (items.length >= MAX_STEPS) return;
		for (const item of items) item.open = false;
		items.push({ id: nextId++, open: true, step: emptyStep() });
		commit();
	}

	function move(index: number, to: number) {
		items = moveStep(items, index, to);
		commit();
	}

	function remove(index: number) {
		items = items.filter((_, i) => i !== index);
		commit();
	}

	const checkLabel: Record<CheckType, () => string> = {
		contains: () => m.deviceform_steps_check_contains(),
		not_contains: () => m.deviceform_steps_check_not_contains(),
		json: () => m.deviceform_steps_check_json()
	};
	const sourceLabel: Record<Source, () => string> = {
		json: () => m.deviceform_steps_from_json(),
		header: () => m.deviceform_steps_from_header(),
		regex: () => m.deviceform_steps_from_regex()
	};

	function issueText(issue: StepIssue): string {
		switch (issue.kind) {
			case 'url_missing':
				return m.deviceform_steps_url_required();
			case 'bad_name':
				return m.deviceform_steps_bad_name({ name: issue.name });
			case 'unknown_variable':
				return m.deviceform_steps_unknown_variable({ token: `{{${issue.name}}}` });
		}
	}

	function title(item: Item, index: number): string {
		return item.step.name.trim() || m.deviceform_steps_untitled({ number: String(index + 1) });
	}

	const iconButton =
		'inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-surface-2 hover:text-ink disabled:pointer-events-none disabled:opacity-40';
	const subHeading = 'flex flex-wrap items-center justify-between gap-x-4 gap-y-1';
</script>

<fieldset class="grid gap-2" aria-describedby={option.help ? `${id}-help` : undefined}>
	<legend class="mb-1 flex w-full flex-wrap items-center justify-between gap-x-4 gap-y-1">
		<span class="text-sm font-semibold text-ink">
			{option.label}{#if option.required}<span class="ml-0.5 text-warning" aria-hidden="true">*</span>{/if}
		</span>
		<Button variant="secondary" size="sm" onclick={add} disabled={items.length >= MAX_STEPS}>
			<Plus class="size-4" aria-hidden="true" />{m.deviceform_steps_add()}
		</Button>
	</legend>
	{#if option.help}
		<p id={`${id}-help`} class="text-[0.8125rem] text-ink-2">{option.help}</p>
	{/if}
	<p class="text-[0.8125rem] text-ink-2">
		{m.deviceform_steps_secrets_hint({ username: '{{secret.username}}', password: '{{secret.password}}', token: '{{secret.token}}' })}
	</p>

	{#if items.length === 0}
		<p class="text-[0.8125rem] text-ink-3">{m.deviceform_steps_empty()}</p>
		{#if error}<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{error}</p>{/if}
	{/if}

	<ol class="grid gap-2">
		{#each items as item, index (item.id)}
			{@const found = issues.get(index) ?? []}
			{@const body = `${id}-step-${item.id}`}
			{@const number = String(index + 1)}
			<li class="min-w-0 rounded-lg border border-line bg-surface">
				<div class="flex items-center gap-1 p-1">
					<button
						type="button"
						class="flex min-h-10 min-w-0 flex-1 items-center gap-3 rounded-md px-2 py-1 text-left hover:bg-surface-2"
						aria-expanded={item.open}
						aria-controls={body}
						onclick={() => (item.open = !item.open)}
					>
						<span class="tnum inline-flex size-6 shrink-0 items-center justify-center rounded-full border border-line text-[0.75rem] font-semibold text-ink-2" aria-hidden="true">{number}</span>
						<span class="min-w-0 flex-1">
							<span class="block truncate text-sm font-semibold text-ink">{title(item, index)}</span>
							<span class="block truncate font-mono text-[0.75rem] text-ink-2">{item.step.method} {item.step.url.trim() || '/'}</span>
						</span>
						{#if found.length > 0}
							<StatusMark glyph="bang" tone="warning" label={m.deviceform_steps_needs_fixing()} />
						{/if}
						<ChevronDown class={`size-4 shrink-0 text-ink-2 transition-transform motion-reduce:transition-none ${item.open ? 'rotate-180' : ''}`} aria-hidden="true" />
					</button>
					<button type="button" class={iconButton} aria-label={m.deviceform_steps_move_up({ number })} disabled={index === 0} onclick={() => move(index, index - 1)}>
						<ArrowUp class="size-4" aria-hidden="true" />
					</button>
					<button
						type="button"
						class={iconButton}
						aria-label={m.deviceform_steps_move_down({ number })}
						disabled={index === items.length - 1}
						onclick={() => move(index, index + 1)}
					>
						<ArrowDown class="size-4" aria-hidden="true" />
					</button>
					<button type="button" class={iconButton} aria-label={m.deviceform_steps_remove({ number })} onclick={() => remove(index)}>
						<Trash2 class="size-4" aria-hidden="true" />
					</button>
				</div>

				{#if item.open}
					<div id={body} class="grid gap-4 border-t border-line px-3 py-3 sm:grid-cols-[minmax(0,1fr)_8rem_10rem]">
						<Field label={m.deviceform_steps_name()} for={`${body}-name`}>
							<input
								id={`${body}-name`}
								class="input"
								type="text"
								autocomplete="off"
								placeholder={m.deviceform_steps_name_placeholder()}
								value={item.step.name}
								oninput={(e) => edit(() => (item.step.name = e.currentTarget.value))}
							/>
						</Field>
						<Field label={m.deviceform_steps_method()} for={`${body}-method`}>
							<select
								id={`${body}-method`}
								class="input"
								value={item.step.method}
								onchange={(e) => edit(() => (item.step.method = e.currentTarget.value))}
							>
								{#each METHODS as method (method)}
									<option value={method}>{method}</option>
								{/each}
							</select>
						</Field>
						<Field label={m.deviceform_steps_status()} for={`${body}-status`} help={m.deviceform_steps_status_help()}>
							<input
								id={`${body}-status`}
								class="input tnum"
								type="text"
								inputmode="numeric"
								autocomplete="off"
								placeholder="200-299"
								value={item.step.expect_status}
								oninput={(e) => edit(() => (item.step.expect_status = e.currentTarget.value))}
							/>
						</Field>
						<Field
							label={m.deviceform_steps_url()}
							for={`${body}-url`}
							help={m.deviceform_steps_url_help({ token: '{{name}}' })}
							required
							class="sm:col-span-3"
						>
							<input
								id={`${body}-url`}
								class="input font-mono text-[0.8125rem]"
								type="text"
								autocomplete="off"
								autocapitalize="off"
								spellcheck="false"
								placeholder="/login"
								value={item.step.url}
								oninput={(e) => edit(() => (item.step.url = e.currentTarget.value))}
								aria-invalid={found.some((issue) => issue.kind === 'url_missing') ? 'true' : undefined}
							/>
						</Field>

						<!-- Headers -->
						<div class="grid gap-2 sm:col-span-3">
							<div class={subHeading}>
								<span class="text-sm font-semibold text-ink">{m.deviceform_steps_headers()}</span>
								<Button variant="ghost" size="sm" onclick={() => edit(() => item.step.headers.push({ name: '', value: '' }))}>
									<Plus class="size-4" aria-hidden="true" />{m.deviceform_steps_header_add()}
								</Button>
							</div>
							{#each item.step.headers as header, h (h)}
								<div class="grid items-center gap-2 sm:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_auto]">
									<input
										class="input font-mono text-[0.8125rem]"
										type="text"
										autocomplete="off"
										autocapitalize="off"
										spellcheck="false"
										aria-label={m.deviceform_steps_header_name()}
										placeholder="Content-Type"
										value={header.name}
										oninput={(e) => edit(() => (header.name = e.currentTarget.value))}
									/>
									<input
										class="input font-mono text-[0.8125rem]"
										type="text"
										autocomplete="off"
										autocapitalize="off"
										spellcheck="false"
										aria-label={m.deviceform_steps_header_value()}
										placeholder="application/json"
										value={header.value}
										oninput={(e) => edit(() => (header.value = e.currentTarget.value))}
									/>
									<button
										type="button"
										class={`${iconButton} max-sm:justify-self-end`}
										aria-label={m.deviceform_steps_header_remove()}
										onclick={() => edit(() => item.step.headers.splice(h, 1))}
									>
										<X class="size-4" aria-hidden="true" />
									</button>
								</div>
							{/each}
						</div>

						<Field label={m.deviceform_steps_body()} for={`${body}-body`} help={m.deviceform_steps_body_help({ token: '{{secret.password|json}}' })} class="sm:col-span-3">
							<textarea
								id={`${body}-body`}
								class="input min-h-24 font-mono text-[0.8125rem]"
								rows="4"
								autocomplete="off"
								autocapitalize="off"
								spellcheck="false"
								value={item.step.body}
								oninput={(e) => edit(() => (item.step.body = e.currentTarget.value))}
							></textarea>
						</Field>

						<!-- Checks on the response -->
						<div class="grid gap-2 sm:col-span-3">
							<div class={subHeading}>
								<span class="text-sm font-semibold text-ink">{m.deviceform_steps_checks()}</span>
								<Button variant="ghost" size="sm" onclick={() => edit(() => item.step.assertions.push(emptyCheck()))}>
									<Plus class="size-4" aria-hidden="true" />{m.deviceform_steps_check_add()}
								</Button>
							</div>
							{#each item.step.assertions as check, c (c)}
								<div class="grid items-center gap-2 sm:grid-cols-[11rem_minmax(0,1fr)_minmax(0,1fr)_auto]">
									<select
										class="input"
										aria-label={m.deviceform_steps_check_type()}
										value={check.type}
										onchange={(e) => edit(() => (check.type = e.currentTarget.value as CheckType))}
									>
										{#each CHECK_TYPES as type (type)}
											<option value={type}>{checkLabel[type]()}</option>
										{/each}
									</select>
									{#if check.type === 'json'}
										<input
											class="input font-mono text-[0.8125rem]"
											type="text"
											autocomplete="off"
											spellcheck="false"
											aria-label={m.deviceform_steps_check_path()}
											placeholder="$.status"
											value={check.path}
											oninput={(e) => edit(() => (check.path = e.currentTarget.value))}
										/>
									{/if}
									<input
										class={`input ${check.type === 'json' ? 'font-mono text-[0.8125rem]' : 'sm:col-span-2'}`}
										type="text"
										autocomplete="off"
										spellcheck="false"
										aria-label={check.type === 'json' ? m.deviceform_steps_check_expected() : m.deviceform_steps_check_text()}
										placeholder={check.type === 'json' ? 'ok' : m.deviceform_steps_check_text_placeholder()}
										value={check.value}
										oninput={(e) => edit(() => (check.value = e.currentTarget.value))}
									/>
									<button
										type="button"
										class={`${iconButton} max-sm:justify-self-end`}
										aria-label={m.deviceform_steps_check_remove()}
										onclick={() => edit(() => item.step.assertions.splice(c, 1))}
									>
										<X class="size-4" aria-hidden="true" />
									</button>
								</div>
							{/each}
						</div>

						<!-- Values kept for the next steps -->
						<div class="grid gap-2 sm:col-span-3">
							<div class={subHeading}>
								<span class="text-sm font-semibold text-ink">{m.deviceform_steps_extractions()}</span>
								<Button variant="ghost" size="sm" onclick={() => edit(() => item.step.extract.push(emptyExtraction()))}>
									<Plus class="size-4" aria-hidden="true" />{m.deviceform_steps_extraction_add()}
								</Button>
							</div>
							<p class="text-[0.8125rem] text-ink-2">{m.deviceform_steps_extraction_help({ token: '{{name}}' })}</p>
							{#each item.step.extract as extraction, x (x)}
								<div class="grid items-center gap-2 sm:grid-cols-[minmax(0,10rem)_11rem_minmax(0,1fr)_auto]">
									<input
										class="input font-mono text-[0.8125rem]"
										type="text"
										autocomplete="off"
										autocapitalize="off"
										spellcheck="false"
										aria-label={m.deviceform_steps_extraction_name()}
										placeholder="token"
										value={extraction.name}
										oninput={(e) => edit(() => (extraction.name = e.currentTarget.value))}
									/>
									<select
										class="input"
										aria-label={m.deviceform_steps_extraction_from()}
										value={extraction.from}
										onchange={(e) => edit(() => (extraction.from = e.currentTarget.value as Source))}
									>
										{#each SOURCES as source (source)}
											<option value={source}>{sourceLabel[source]()}</option>
										{/each}
									</select>
									<input
										class="input font-mono text-[0.8125rem]"
										type="text"
										autocomplete="off"
										autocapitalize="off"
										spellcheck="false"
										aria-label={m.deviceform_steps_extraction_expression()}
										placeholder={extraction.from === 'header' ? 'X-Request-Id' : extraction.from === 'regex' ? 'id=(\\d+)' : '$.token'}
										value={extraction.expression}
										oninput={(e) => edit(() => (extraction.expression = e.currentTarget.value))}
									/>
									<button
										type="button"
										class={`${iconButton} max-sm:justify-self-end`}
										aria-label={m.deviceform_steps_extraction_remove()}
										onclick={() => edit(() => item.step.extract.splice(x, 1))}
									>
										<X class="size-4" aria-hidden="true" />
									</button>
								</div>
							{/each}
						</div>
					</div>
				{/if}

				{#if found.length > 0}
					<div class="grid gap-1 px-3 pb-2">
						{#each found as issue, i (i)}
							<p class="text-[0.8125rem] font-medium text-warning-ink" role="alert">{issueText(issue)}</p>
						{/each}
					</div>
				{/if}
			</li>
		{/each}
	</ol>

	{#if items.length >= MAX_STEPS}
		<p class="text-[0.8125rem] text-ink-2">{m.deviceform_steps_limit({ max: String(MAX_STEPS) })}</p>
	{/if}
</fieldset>
