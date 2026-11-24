<script lang="ts">
	/**
	 * Settings → Wall weather. The outside temperature and a weather icon beside
	 * the wall's date, for every wall. Off until an administrator picks a city
	 * (searched by name on Open-Meteo, by the server: the browser never calls it),
	 * with °C or °F. Each change is saved at once. Anyone may read it; only
	 * administrators change it.
	 *
	 * The panel stays on screen in every state: a failed read, search or save
	 * is named with its reason, and a saved city the weather service has not
	 * answered for yet says so (the wall shows nothing until it does).
	 */
	import { untrack } from 'svelte';
	import { Check, Loader2, MapPin, Search } from 'lucide-svelte';
	import { getWallWeather, searchWallWeatherPlaces, setWallWeather } from '#lib/api/weather.js';
	import type { WallWeather, WallWeatherPlace } from '#lib/api/types.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ErrorNotice, Field, Panel, Skeleton } from '#lib/ui/index.js';
	import { getLocale } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';

	let setting = $state<WallWeather | null>(null);
	let loading = $state(true);
	let error = $state<unknown>(null);
	/** The change being saved: a search result (`place:<index>`), the unit, or the removal. */
	let pending = $state<string | null>(null);
	let saveError = $state<unknown>(null);
	const saving = $derived(pending !== null);

	let query = $state('');
	let searching = $state(false);
	let results = $state<WallWeatherPlace[] | null>(null);
	let searchError = $state<unknown>(null);

	let reading: AbortController | null = null;

	function load() {
		reading?.abort();
		const controller = new AbortController();
		reading = controller;
		loading = true;
		error = null;
		getWallWeather(controller.signal)
			.then((value) => (setting = value))
			.catch((cause) => {
				if (cause instanceof DOMException && cause.name === 'AbortError') return;
				error = cause;
			})
			.finally(() => {
				if (reading === controller) loading = false;
			});
	}

	$effect(() => {
		untrack(load);
		return () => reading?.abort();
	});

	const UNITS = $derived<{ value: 'c' | 'f'; label: string }[]>([
		{ value: 'c', label: m.settings_wall_weather_unit_c() },
		{ value: 'f', label: m.settings_wall_weather_unit_f() }
	]);

	function describe(place: WallWeatherPlace): string {
		return [place.name, place.region, place.country].filter((part): part is string => !!part).join(', ');
	}

	/** Saves the whole setting; true when the server took it. */
	async function save(place: WallWeatherPlace | null, unit: 'c' | 'f', what: string): Promise<boolean> {
		pending = what;
		saveError = null;
		try {
			// Only the fields the server reads, as a plain object (not the list's state proxy).
			const body = place
				? { name: place.name, region: place.region ?? null, country: place.country ?? null, latitude: place.latitude, longitude: place.longitude }
				: null;
			setting = await setWallWeather({ place: body, unit });
			return true;
		} catch (cause) {
			saveError = cause;
			return false;
		} finally {
			pending = null;
		}
	}

	async function search(event: SubmitEvent) {
		event.preventDefault();
		const text = query.trim();
		if (text.length < 2 || searching) return;
		searching = true;
		searchError = null;
		results = null;
		try {
			results = await searchWallWeatherPlaces(text, getLocale());
		} catch (cause) {
			searchError = cause;
		} finally {
			searching = false;
		}
	}

	async function pick(place: WallWeatherPlace, index: number) {
		if (await save(place, setting?.unit ?? 'c', `place:${index}`)) {
			results = null;
			query = '';
		}
	}

	const CHIP = 'min-h-10 rounded-[var(--radius-plate)] border px-3 py-1 text-[0.8125rem] transition-colors';
	const CHIP_ON = 'border-signal bg-signal-soft font-semibold text-ink';
	const CHIP_OFF = 'border-line text-ink-2 hover:bg-surface-2';
</script>

<Panel id="wall-weather" title={m.settings_wall_weather_title()} description={m.settings_wall_weather_description()}>
	{#if error}
		<ErrorNotice {error} title={m.settings_wall_weather_read_error()} onretry={load} />
	{:else if loading || !setting}
		<Skeleton class="h-8 w-full" rows={2} />
	{:else}
		<div class="grid gap-5">
			{#if setting.place}
				<!-- The chosen city, as a card: what the wall shows, at a glance. -->
				<div class="grid gap-1.5" aria-live="polite">
					<p class="m-0 text-sm font-semibold text-ink">{m.settings_wall_weather_selected_label()}</p>
					<div class="flex min-h-14 items-center gap-3 rounded-[var(--radius-plate)] border border-signal bg-signal-soft px-3 py-2">
						<MapPin class="size-5 shrink-0 text-ink-2" aria-hidden="true" />
						<div class="grid min-w-0 flex-1">
							<span class="truncate text-sm font-semibold text-ink">{setting.place.name}</span>
							{#if setting.place.region || setting.place.country}
								<span class="truncate text-[0.8125rem] text-ink-2">{[setting.place.region, setting.place.country].filter(Boolean).join(', ')}</span>
							{/if}
						</div>
						{#if setting.current}
							<span class="tnum shrink-0 text-lg font-semibold text-ink">{setting.current.temperature}°{setting.unit === 'f' ? 'F' : 'C'}</span>
						{/if}
						{#if auth.isAdmin}
							<Button variant="secondary" class="shrink-0" loading={pending === 'remove'} disabled={saving} onclick={() => void save(null, setting?.unit ?? 'c', 'remove')}>{m.settings_wall_weather_remove()}</Button>
						{/if}
					</div>
					{#if !setting.current}
						<p class="m-0 text-[0.8125rem] text-ink-2">{m.settings_wall_weather_no_reading()}</p>
					{/if}
				</div>
			{:else}
				<p class="m-0 text-sm text-ink-2" aria-live="polite">{m.settings_wall_weather_none()}</p>
			{/if}

			{#if saveError}<ErrorNotice error={saveError} title={m.settings_wall_weather_save_error()} />{/if}

			{#if auth.isAdmin}
				<div class="grid gap-3">
					<form onsubmit={search} novalidate>
						<Field label={setting.place ? m.settings_wall_weather_change_label() : m.settings_wall_weather_search_label()} for="wall-weather-query" help={m.settings_wall_weather_search_help()}>
							<!-- The input and its button on one line, both 2.5rem high. -->
							<div class="flex items-stretch gap-2">
								<input
									id="wall-weather-query"
									type="search"
									class="input h-10 min-w-0 flex-1"
									bind:value={query}
									maxlength="80"
									autocomplete="off"
									disabled={saving}
								/>
								<Button type="submit" variant="secondary" class="shrink-0" loading={searching} disabled={query.trim().length < 2 || saving}>
									{#if !searching}<Search class="size-4" aria-hidden="true" />{/if}
									{m.settings_wall_weather_search_button()}
								</Button>
							</div>
						</Field>
					</form>

					{#if searchError}
						<ErrorNotice error={searchError} title={m.settings_wall_weather_search_error()} />
					{:else if results}
						{#if results.length === 0}
							<p class="m-0 text-sm text-ink-2" role="status">{m.settings_wall_weather_no_results()}</p>
						{:else}
							<ul class="m-0 grid list-none gap-1.5 p-0" aria-label={m.settings_wall_weather_results_label()}>
								<!-- Keyed by position: two hits can share their rounded coordinates. -->
								{#each results as place, i (i)}
									{@const busy = pending === `place:${i}`}
									<li>
										<button
											type="button"
											class="flex min-h-10 w-full items-center gap-2.5 rounded-[var(--radius-plate)] border border-line px-3 py-1.5 text-left text-sm text-ink hover:bg-surface-2 disabled:opacity-60"
											disabled={saving}
											aria-busy={busy}
											onclick={() => void pick(place, i)}
										>
											{#if busy}
												<Loader2 class="size-4 shrink-0 animate-spin text-ink-2" aria-hidden="true" />
											{:else}
												<MapPin class="size-4 shrink-0 text-ink-3" aria-hidden="true" />
											{/if}
											<span class="min-w-0 truncate">{describe(place)}</span>
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					{/if}
				</div>

				<div class="grid gap-2" role="group" aria-labelledby="wall-weather-unit">
					<p id="wall-weather-unit" class="m-0 text-sm font-semibold text-ink">{m.settings_wall_weather_unit_label()}</p>
					<div class="flex flex-wrap gap-1.5" role="radiogroup" aria-labelledby="wall-weather-unit">
						{#each UNITS as option (option.value)}
							{@const selected = setting.unit === option.value}
							<button
								type="button"
								role="radio"
								aria-checked={selected}
								class={`${CHIP} flex items-center gap-1.5 ${selected ? CHIP_ON : CHIP_OFF}`}
								disabled={saving}
								onclick={() => void save(setting?.place ?? null, option.value, 'unit')}
							>
								{#if selected}<Check class="size-3.5" aria-hidden="true" />{/if}
								{option.label}
							</button>
						{/each}
					</div>
				</div>

			{:else}
				<p class="m-0 text-[0.8125rem] text-ink-3">{m.settings_wall_weather_admin_only()}</p>
			{/if}
		</div>
	{/if}
</Panel>
