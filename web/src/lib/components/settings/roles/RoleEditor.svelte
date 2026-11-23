<script lang="ts">
	/**
	 * Create or edit one custom role: a name, a description and the
	 * permissions it holds, picked from the server's catalog. Permissions the
	 * catalog marks as implied (`view`) are always on.
	 */
	import type { CustomRole, Permission, PermissionInfo } from '#lib/api/types.js';
	import { createCustomRole, updateCustomRole } from '#lib/api/roles.js';
	import { Button, ErrorNotice, Field } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { permissionHelp, permissionLabel } from './permissionLabels.js';

	interface Props {
		/** `null` creates a role. */
		role: CustomRole | null;
		catalog: PermissionInfo[];
		oncancel: () => void;
		onsaved: (role: CustomRole) => void;
	}
	let { role, catalog, oncancel, onsaved }: Props = $props();

	const initial = () => role;
	let name = $state(initial()?.name ?? '');
	let description = $state(initial()?.description ?? '');
	let chosen = $state<Permission[]>([...(initial()?.permissions ?? [])]);

	let nameError = $state<string | null>(null);
	let saving = $state(false);
	let error = $state<unknown>(null);

	function isOn(info: PermissionInfo): boolean {
		return info.implied || chosen.includes(info.id);
	}

	function toggle(id: Permission, on: boolean) {
		chosen = on ? [...chosen.filter((x) => x !== id), id] : chosen.filter((x) => x !== id);
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		nameError = name.trim() ? null : m.w6roles_editor_err_name();
		if (nameError) return;
		saving = true;
		try {
			// Only what the catalog offers: a permission the server dropped is not sent back.
			const known = new Set(catalog.map((info) => info.id));
			const payload = {
				name: name.trim(),
				description: description.trim(),
				permissions: chosen.filter((id) => known.has(id))
			};
			onsaved(role ? await updateCustomRole(role.id, payload) : await createCustomRole(payload));
		} catch (cause) {
			error = cause;
		} finally {
			saving = false;
		}
	}

	const idBase = $derived(`role-${role?.id ?? 'new'}`);
</script>

<form class="grid gap-5 rounded-[var(--radius-card)] border border-line-strong bg-surface-2/40 p-3 sm:p-4" onsubmit={submit} novalidate>
	<h3 class="text-sm font-semibold text-ink">{role ? m.w6roles_editor_edit_title({ name: role.name }) : m.w6roles_editor_new_title()}</h3>

	<div class="grid gap-4 sm:grid-cols-2">
		<Field label={m.w6roles_editor_name()} for={`${idBase}-name`} error={nameError} required>
			<input
				id={`${idBase}-name`}
				type="text"
				class="input"
				maxlength="64"
				bind:value={name}
				autocomplete="off"
				disabled={saving}
				aria-invalid={nameError ? 'true' : undefined}
				oninput={() => (nameError = null)}
			/>
		</Field>
		<Field label={m.w6roles_editor_description()} for={`${idBase}-description`}>
			<input id={`${idBase}-description`} type="text" class="input" maxlength="500" bind:value={description} autocomplete="off" disabled={saving} />
		</Field>
	</div>

	<fieldset class="grid gap-1" disabled={saving}>
		<legend class="mb-1 text-sm font-semibold text-ink">{m.w6roles_editor_permissions()}</legend>
		<ul class="grid gap-1 rounded-lg border border-line bg-canvas p-1">
			{#each catalog as info (info.id)}
				<li>
					<label class={`flex min-h-10 items-start gap-3 rounded-md px-2 py-2 text-sm ${info.implied ? '' : 'cursor-pointer hover:bg-surface-2'}`}>
						<input
							type="checkbox"
							class="mt-0.5 size-4 shrink-0"
							checked={isOn(info)}
							disabled={info.implied}
							onchange={(e) => toggle(info.id, e.currentTarget.checked)}
						/>
						<span class="grid min-w-0 flex-1 gap-0.5">
							<span class="font-semibold text-ink">
								{permissionLabel(info.id, info)}
								{#if info.implied}<span class="font-normal text-ink-2"> · {m.w6roles_editor_implied()}</span>{/if}
							</span>
							<span class="text-ink-2 [overflow-wrap:anywhere]">{permissionHelp(info)}</span>
						</span>
					</label>
				</li>
			{/each}
		</ul>
		<p class="text-sm text-ink-2">{m.w6roles_section_admin_only()}</p>
	</fieldset>

	{#if error}
		<ErrorNotice {error} title={m.w6roles_editor_save_error()} />
	{/if}

	<div class="flex flex-wrap items-center gap-2">
		<Button type="submit" variant="primary" loading={saving}>{role ? m.w6roles_editor_save() : m.w6roles_editor_create()}</Button>
		<Button variant="ghost" onclick={oncancel} disabled={saving}>{m.w6roles_editor_cancel()}</Button>
	</div>
</form>
