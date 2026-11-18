<script lang="ts">
	/**
	 * The instance's accent colour, in Settings › Appearance (administrators):
	 * a few preset hues and a custom one. It colours the solid buttons and the
	 * focus ring for everyone; the status colours never change. The page wears
	 * the pick at once and the choice is saved on its own, like the theme.
	 */
	import { untrack } from 'svelte';
	import { Check } from 'lucide-svelte';
	import { updateInstanceSettings } from '#lib/api/instance.js';
	import { ACCENT_PALETTE, deriveAccent, inkOn, parseAccent } from '#lib/accent.js';
	import { m } from '#lib/paraglide/messages.js';
	import { instanceStore } from '#lib/stores/instance.svelte.js';
	import { ErrorNotice, Plate } from '#lib/ui/index.js';

	const NAMES: Record<string, () => string> = {
		default: () => m.settings_accent_name_default(),
		blue: () => m.settings_accent_name_blue(),
		sky: () => m.settings_accent_name_sky(),
		teal: () => m.settings_accent_name_teal(),
		indigo: () => m.settings_accent_name_indigo(),
		violet: () => m.settings_accent_name_violet(),
		pink: () => m.settings_accent_name_pink(),
		orange: () => m.settings_accent_name_orange(),
		slate: () => m.settings_accent_name_slate()
	};

	const current = $derived(instanceStore.accent);
	const isPreset = $derived(ACCENT_PALETTE.some((p) => p.hex === current));
	const derived = $derived(deriveAccent(current));
	// A colour input needs a value even for the default look.
	const customValue = $derived(current || '#2563eb');

	let busy = $state(false);
	let saved = $state(false);
	let error = $state<unknown>(null);
	/** What the server has, to put back if a save fails. */
	let confirmed = '';
	$effect(() => {
		if (instanceStore.settingsLoaded) confirmed = untrack(() => instanceStore.accent);
	});

	async function save(raw: string) {
		const next = parseAccent(raw);
		if (busy) return;
		busy = true;
		error = null;
		saved = false;
		instanceStore.setAccent(next);
		try {
			const settings = await updateInstanceSettings({ accent: next });
			confirmed = parseAccent(settings.accent);
			instanceStore.setAccent(confirmed);
			saved = true;
		} catch (cause) {
			error = cause;
			instanceStore.setAccent(confirmed);
		} finally {
			busy = false;
		}
	}
</script>

<div class="grid gap-3 border-t border-line pt-4">
	<div>
		<span id="settings-accent-label" class="text-sm font-semibold text-ink">{m.settings_accent_label()}</span>
		<p class="mt-0.5 text-[0.8125rem] text-ink-2">{m.settings_accent_hint()}</p>
	</div>
	<div role="radiogroup" aria-labelledby="settings-accent-label" class="flex flex-wrap items-center gap-2">
		{#each ACCENT_PALETTE as swatch (swatch.id)}
			{@const checked = swatch.hex === current}
			<label
				class="relative flex size-10 cursor-pointer items-center justify-center rounded-full border border-line-strong has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-focus {checked ? 'ring-2 ring-ink ring-offset-2 ring-offset-surface' : ''} {swatch.hex ? '' : 'bg-ink'}"
				style={swatch.hex ? `background-color: ${swatch.hex}` : undefined}
				title={NAMES[swatch.id]()}
			>
				<input
					type="radio"
					name="instance-accent"
					value={swatch.hex}
					{checked}
					disabled={busy}
					class="sr-only"
					aria-label={NAMES[swatch.id]()}
					onchange={() => void save(swatch.hex)}
				/>
				{#if checked}<Check class={swatch.hex ? 'size-4' : 'size-4 text-surface'} style={swatch.hex ? `color: ${inkOn(swatch.hex)}` : undefined} strokeWidth={3} aria-hidden="true" />{/if}
			</label>
		{/each}
		<label
			class="relative flex h-10 cursor-pointer items-center gap-2 rounded-full border border-line-strong bg-surface-2 px-3 text-sm text-ink has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-focus {!isPreset ? 'ring-2 ring-ink ring-offset-2 ring-offset-surface' : ''}"
		>
			<input
				type="color"
				value={customValue}
				disabled={busy}
				class="size-6 cursor-pointer rounded border-0 bg-transparent p-0"
				aria-label={m.settings_accent_custom()}
				oninput={(event) => instanceStore.setAccent(event.currentTarget.value)}
				onchange={(event) => void save(event.currentTarget.value)}
			/>
			<span>{m.settings_accent_custom()}</span>
			{#if !isPreset}<Check class="size-4" strokeWidth={3} aria-hidden="true" />{/if}
		</label>
	</div>
	{#if derived}
		<!-- What the pick becomes in each light, with the ink laid on it. -->
		<div class="flex flex-wrap items-center gap-2 text-sm font-semibold" aria-hidden="true">
			<span class="inline-flex h-10 items-center rounded-lg px-4" style={`background-color: ${derived.lightFill}; color: ${derived.lightOn}`}>
				{m.settings_appearance_theme_light()}
			</span>
			<span class="inline-flex h-10 items-center rounded-lg px-4" style={`background-color: ${derived.darkFill}; color: ${derived.darkOn}`}>
				{m.settings_appearance_theme_dark()}
			</span>
		</div>
	{/if}
	<div class="flex flex-wrap items-center gap-3" aria-live="polite">
		{#if error}<ErrorNotice {error} />{/if}
		{#if saved && !error}<Plate tone="signal" draw>{m.settings_general_saved()}</Plate>{/if}
	</div>
</div>
