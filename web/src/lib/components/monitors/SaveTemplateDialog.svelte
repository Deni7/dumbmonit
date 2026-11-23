<script lang="ts">
	/**
	 * "Save as template" of a monitor's menu: a name, then the server builds the
	 * template from what it stores for the monitor (type, options and tags
	 * without any secret, interval, notification channels). Once saved, the
	 * dialog offers to add a monitor from it straight away.
	 */
	import { tick } from 'svelte';
	import { X } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Target } from '#lib/api/index.js';
	import { createMonitorTemplate, type MonitorTemplate } from '#lib/api/monitorTemplates.js';
	import { Button, ErrorNotice, Field } from '#lib/ui/index.js';

	interface Props {
		target: Target;
	}

	let { target }: Props = $props();

	let open = $state(false);
	let dialog = $state<HTMLDivElement | null>(null);
	let returnFocus: HTMLElement | null = null;
	let name = $state('');
	let busy = $state(false);
	let error = $state<unknown>(null);
	let saved = $state<MonitorTemplate | null>(null);

	export function show() {
		returnFocus = (document.activeElement as HTMLElement | null) ?? null;
		name = target.name;
		error = null;
		saved = null;
		open = true;
		void tick().then(() => document.getElementById('template-name')?.focus());
	}

	function close() {
		open = false;
		returnFocus?.focus();
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!name.trim() || busy) return;
		busy = true;
		error = null;
		try {
			saved = await createMonitorTemplate(name.trim(), target.id);
			// The form (and the focused button) is gone: keep focus in the dialog, so Tab and Escape still work.
			void tick().then(() => dialog?.focus());
		} catch (cause) {
			error = cause;
		} finally {
			busy = false;
		}
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			close();
		} else if (event.key === 'Tab' && dialog) {
			const items = [...dialog.querySelectorAll<HTMLElement>('a[href], button:not([disabled]), input:not([disabled])')];
			if (items.length === 0) return;
			const first = items[0];
			const final = items[items.length - 1];
			const active = document.activeElement;
			if (event.shiftKey && (active === first || active === dialog)) {
				event.preventDefault();
				final.focus();
			} else if (!event.shiftKey && active === final) {
				event.preventDefault();
				first.focus();
			}
		}
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-[60] bg-black/50" aria-hidden="true" onclick={close}></div>
	<div
		bind:this={dialog}
		role="dialog"
		aria-modal="true"
		aria-labelledby="save-template-title"
		tabindex="-1"
		{onkeydown}
		class="fixed inset-x-0 bottom-0 z-[62] max-h-[92dvh] overflow-y-auto rounded-t-[var(--radius-card)] border border-line bg-surface p-5 pb-[calc(1.25rem+var(--safe-bottom))] text-ink shadow-float outline-none sm:inset-x-auto sm:top-1/2 sm:bottom-auto sm:left-1/2 sm:w-[28rem] sm:max-w-[calc(100vw-2rem)] sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-[var(--radius-card)] sm:pb-5"
	>
		<div class="flex items-start justify-between gap-3">
			<h2 id="save-template-title" class="min-w-0 text-lg font-semibold">{m.w5monitors_template_save_title()}</h2>
			<button
				type="button"
				class="-mt-1.5 -mr-2 inline-flex size-10 shrink-0 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink"
				aria-label={m.w5monitors_bulkedit_close()}
				onclick={close}
			>
				<X class="size-4" aria-hidden="true" />
			</button>
		</div>

		{#if saved}
			<p class="mt-3 text-sm text-ink" role="status">{m.w5monitors_template_saved({ name: saved.name })}</p>
			<div class="mt-5 flex flex-wrap justify-end gap-2">
				<Button variant="ghost" onclick={close}>{m.w5monitors_template_done()}</Button>
				<Button variant="secondary" href={`/targets/new?kind=${encodeURIComponent(saved.kind)}&template=${saved.id}`}>
					{m.w5monitors_template_add_from()}
				</Button>
			</div>
		{:else}
			<p class="mt-1 text-sm text-ink-2">{m.w5monitors_template_save_lead()}</p>
			{#if error}
				<ErrorNotice {error} title={m.w5monitors_template_save_error()} class="mt-4" />
			{/if}
			<form class="mt-4 grid gap-4" onsubmit={save}>
				<Field label={m.w5monitors_template_name_label()} for="template-name" required>
					<input id="template-name" class="input" autocomplete="off" maxlength="80" bind:value={name} />
				</Field>
				<div class="flex flex-wrap justify-end gap-2">
					<Button variant="ghost" onclick={close}>{m.w5monitors_bulkedit_cancel()}</Button>
					<Button type="submit" variant="primary" loading={busy} disabled={!name.trim()}>{m.w5monitors_template_save()}</Button>
				</div>
			</form>
		{/if}
	</div>
{/if}
