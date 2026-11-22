<script lang="ts">
	/**
	 * "Translations" of a saved status page: the language its interface falls
	 * back to, and, per language, overrides of the texts the admin wrote (title,
	 * description, footer, service and group names). Saved on its own, apart
	 * from the page form. Incident messages are never translated.
	 */
	import { m } from '#lib/paraglide/messages.js';
	import { getStatusPageLanguages, putStatusPageLanguages, type StatusPageTranslation } from '#lib/api/index.js';
	import { locales } from '#lib/paraglide/runtime.js';
	import { Button, ErrorNotice, Field } from '#lib/ui/index.js';
	import { X } from 'lucide-svelte';

	interface Props {
		pageId: number;
		idPrefix: string;
		/** The service and group names as written on the page. */
		names: string[];
		disabled?: boolean;
	}

	let { pageId, idPrefix, names, disabled = false }: Props = $props();

	let defaultLang = $state('en');
	let translations = $state<StatusPageTranslation[]>([]);
	let active = $state<string | null>(null);
	let addChoice = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let saved = $state(false);
	let error = $state<unknown>(null);

	function nativeName(locale: string): string {
		try {
			const name = new Intl.DisplayNames([locale], { type: 'language' }).of(locale) ?? locale;
			return name.charAt(0).toLocaleUpperCase(locale) + name.slice(1);
		} catch {
			return locale;
		}
	}

	$effect(() => {
		const id = pageId;
		const controller = new AbortController();
		loading = true;
		getStatusPageLanguages(id, controller.signal)
			.then((found) => {
				defaultLang = found.default_lang || 'en';
				translations = found.translations.map((t) => ({ ...t, labels: { ...t.labels } }));
				active = translations[0]?.lang ?? null;
				error = null;
			})
			.catch((cause) => {
				if (!controller.signal.aborted) error = cause;
			})
			.finally(() => {
				if (!controller.signal.aborted) loading = false;
			});
		return () => controller.abort();
	});

	const available = $derived(locales.filter((l) => !translations.some((t) => t.lang === l)));
	const current = $derived(translations.find((t) => t.lang === active) ?? null);

	function add() {
		if (!addChoice) return;
		translations = [...translations, { lang: addChoice, title: '', description: '', footer_text: '', labels: {} }];
		active = addChoice;
		addChoice = '';
		saved = false;
	}

	function remove(lang: string) {
		translations = translations.filter((t) => t.lang !== lang);
		if (active === lang) active = translations[0]?.lang ?? null;
		saved = false;
	}

	function setLabel(lang: string, name: string, value: string) {
		translations = translations.map((t) => (t.lang === lang ? { ...t, labels: { ...t.labels, [name]: value } } : t));
		saved = false;
	}

	function patch(lang: string, change: Partial<StatusPageTranslation>) {
		translations = translations.map((t) => (t.lang === lang ? { ...t, ...change } : t));
		saved = false;
	}

	async function save() {
		saving = true;
		saved = false;
		try {
			const stored = await putStatusPageLanguages(pageId, { default_lang: defaultLang === 'en' ? '' : defaultLang, translations });
			translations = stored.translations.map((t) => ({ ...t, labels: { ...t.labels } }));
			if (!translations.some((t) => t.lang === active)) active = translations[0]?.lang ?? null;
			error = null;
			saved = true;
		} catch (cause) {
			error = cause;
		} finally {
			saving = false;
		}
	}
</script>

<div class="grid gap-4" aria-busy={loading}>
	<p class="text-[0.8125rem] text-ink-2">{m.w5calendar_langs_intro()}</p>

	{#if error}
		<ErrorNotice {error} title={loading ? m.w5calendar_langs_error_load() : m.w5calendar_langs_error_save()} />
	{/if}

	<fieldset class="grid gap-4" disabled={disabled || saving || loading}>
		<legend class="sr-only">{m.w5calendar_langs_section()}</legend>
		<Field label={m.w5calendar_langs_default_label()} for="{idPrefix}-lang-default" help={m.w5calendar_langs_default_help()}>
			<select id="{idPrefix}-lang-default" class="input" bind:value={defaultLang} onchange={() => (saved = false)}>
				{#each locales as locale (locale)}
					<option value={locale} lang={locale}>{nativeName(locale)}</option>
				{/each}
			</select>
		</Field>

		<div class="flex flex-wrap items-end gap-2">
			<Field label={m.w5calendar_langs_add_label()} for="{idPrefix}-lang-add" class="min-w-0 flex-1">
				<select id="{idPrefix}-lang-add" class="input" bind:value={addChoice}>
					<option value="">{m.w5calendar_langs_add_placeholder()}</option>
					{#each available as locale (locale)}
						<option value={locale} lang={locale}>{nativeName(locale)}</option>
					{/each}
				</select>
			</Field>
			<Button variant="secondary" disabled={!addChoice} onclick={add}>{m.w5calendar_langs_add()}</Button>
		</div>

		{#if translations.length === 0}
			<p class="text-[0.8125rem] text-ink-2">{m.w5calendar_langs_empty()}</p>
		{:else}
			<div role="group" aria-label={m.w5calendar_langs_tabs_label()} class="flex flex-wrap gap-1.5">
				{#each translations as t (t.lang)}
					<span class={`inline-flex items-center rounded-lg border ${t.lang === active ? 'border-line-strong bg-surface-2 font-semibold text-ink' : 'border-line text-ink-2'}`}>
						<button type="button" class="min-h-10 px-3 text-sm focus-visible:outline-2 focus-visible:outline-signal" aria-pressed={t.lang === active} lang={t.lang} onclick={() => (active = t.lang)}>
							{nativeName(t.lang)}
						</button>
						<button type="button" class="inline-flex size-10 items-center justify-center text-ink-3 hover:text-ink focus-visible:outline-2 focus-visible:outline-signal" aria-label={m.w5calendar_langs_remove({ language: nativeName(t.lang) })} onclick={() => remove(t.lang)}>
							<X class="size-4" aria-hidden="true" />
						</button>
					</span>
				{/each}
			</div>
		{/if}

		{#if current}
			{@const lang = current.lang}
			<div class="grid gap-3" lang={lang}>
				<Field label={m.w5calendar_langs_field_title()} for="{idPrefix}-tr-title">
					<input id="{idPrefix}-tr-title" type="text" class="input" maxlength="120" value={current.title} oninput={(e) => patch(lang, { title: e.currentTarget.value })} />
				</Field>
				<Field label={m.w5calendar_langs_field_description()} for="{idPrefix}-tr-description">
					<textarea id="{idPrefix}-tr-description" class="input min-h-20" maxlength="1000" value={current.description} oninput={(e) => patch(lang, { description: e.currentTarget.value })}></textarea>
				</Field>
				<Field label={m.w5calendar_langs_field_footer()} for="{idPrefix}-tr-footer">
					<input id="{idPrefix}-tr-footer" type="text" class="input" maxlength="280" value={current.footer_text} oninput={(e) => patch(lang, { footer_text: e.currentTarget.value })} />
				</Field>
				{#if names.length > 0}
					<p class="text-sm font-semibold text-ink">{m.w5calendar_langs_names_heading()}</p>
					{#each names as name, i (name)}
						<Field label={name} for="{idPrefix}-tr-name-{i}">
							<input id="{idPrefix}-tr-name-{i}" type="text" class="input" maxlength="80" value={current.labels[name] ?? ''} oninput={(e) => setLabel(lang, name, e.currentTarget.value)} />
						</Field>
					{/each}
				{/if}
			</div>
		{/if}

		<div class="flex flex-wrap items-center gap-3">
			<Button variant="primary" loading={saving} onclick={save}>{m.w5calendar_langs_save()}</Button>
			{#if saved}<span class="text-[0.8125rem] text-ink-2" role="status">{m.w5calendar_langs_saved()}</span>{/if}
		</div>
	</fieldset>
</div>
