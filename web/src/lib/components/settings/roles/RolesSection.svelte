<script lang="ts">
	/**
	 * Settings → Team & access → Custom roles. A custom role is a set of
	 * permissions from the server's fixed catalog (`GET /api/roles/permissions`),
	 * given to an account in place of a built-in role (Users, above). Managing
	 * accounts, roles, teams, tokens, single sign-on, backups and instance
	 * settings never goes into a role. A role still given to an account cannot
	 * be deleted: the server answers 409 and the button says why beforehand.
	 */
	import { Plus, ShieldCheck } from 'lucide-svelte';
	import type { CustomRole, PermissionInfo } from '#lib/api/types.js';
	import { deleteCustomRole, listCustomRoles, listPermissions } from '#lib/api/roles.js';
	import { Button, Confirm, DocLink, EmptyState, ErrorNotice, Panel, Plate, Skeleton } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import RoleEditor from './RoleEditor.svelte';
	import { permissionLabel } from './permissionLabels.js';

	let roles = $state<CustomRole[]>([]);
	let catalog = $state<PermissionInfo[]>([]);
	let loaded = $state(false);
	let loading = $state(true);
	let error = $state<unknown>(null);
	/** A role being edited, `'new'` for the creation form, `null` for none. */
	let editing = $state<CustomRole | 'new' | null>(null);

	async function load(signal?: AbortSignal) {
		loading = true;
		error = null;
		try {
			const [list, permissions] = await Promise.all([listCustomRoles(signal), listPermissions(signal)]);
			roles = Array.isArray(list) ? list : [];
			catalog = Array.isArray(permissions) ? permissions : [];
			loaded = true;
		} catch (cause) {
			if (cause instanceof DOMException && cause.name === 'AbortError') return;
			error = cause;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const controller = new AbortController();
		void load(controller.signal);
		return () => controller.abort();
	});

	function saved() {
		editing = null;
		void load();
	}

	let actionError = $state<unknown>(null);
	let removing = $state<number | null>(null);
	async function remove(role: CustomRole) {
		removing = role.id;
		actionError = null;
		try {
			await deleteCustomRole(role.id);
			if (editing && editing !== 'new' && editing.id === role.id) editing = null;
			await load();
		} catch (cause) {
			actionError = cause;
		} finally {
			removing = null;
		}
	}

	/** The permissions worth listing on a row: `view` goes without saying. */
	function granted(role: CustomRole): string[] {
		const implied = new Set(catalog.filter((info) => info.implied).map((info) => info.id));
		return role.permissions.filter((id) => !implied.has(id));
	}
	const infoOf = (id: string) => catalog.find((info) => info.id === id);
</script>

<Panel id="roles" title={m.w6roles_section_title()} description={m.w6roles_section_description()}>
	{#snippet aside()}
		{#if loaded && editing === null && roles.length > 0}
			<Button variant="secondary" onclick={() => (editing = 'new')}>
				<Plus class="size-4" aria-hidden="true" />
				{m.w6roles_section_add()}
			</Button>
		{/if}
	{/snippet}

	{#if error}
		<ErrorNotice {error} title={m.w6roles_section_load_error()} onretry={() => void load()} />
	{:else if loading && !loaded}
		<Skeleton class="h-10 w-full" rows={3} />
	{:else}
		<div class="grid gap-4">
			{#if actionError}<ErrorNotice error={actionError} title={m.w6roles_section_delete_error()} />{/if}

			{#if editing !== null}
				{#key editing === 'new' ? 'new' : editing.id}
					<RoleEditor role={editing === 'new' ? null : editing} {catalog} oncancel={() => (editing = null)} onsaved={saved} />
				{/key}
			{/if}

			{#if roles.length === 0}
				{#if editing === null}
					<EmptyState compact icon={ShieldCheck} title={m.w6roles_section_empty_title()} description={m.w6roles_section_empty_body()}>
						{#snippet action()}
							<Button variant="primary" onclick={() => (editing = 'new')}>{m.w6roles_section_add()}</Button>
						{/snippet}
					</EmptyState>
				{/if}
			{:else}
				<ul class="grid gap-2">
					{#each roles as role (role.id)}
						{@const extra = granted(role)}
						<li class="rounded-lg border border-line bg-canvas p-3">
							<div class="flex flex-wrap items-start gap-x-4 gap-y-2">
								<div class="min-w-0 flex-[1_1_16rem]">
									<div class="flex flex-wrap items-center gap-2">
										<span class="font-semibold text-ink [overflow-wrap:anywhere]">{role.name}</span>
										<Plate tone="ghost" bare label={m.w6roles_section_users_count({ count: role.users })} />
									</div>
									{#if role.description}<p class="mt-0.5 text-sm text-ink-2 [overflow-wrap:anywhere]">{role.description}</p>{/if}
									<div class="mt-2 flex flex-wrap items-center gap-1.5">
										{#if extra.length === 0}
											<Plate tone="muted" bare label={m.w6roles_section_read_only()} />
										{:else}
											{#each extra as id (id)}
												<Plate tone="info" bare label={permissionLabel(id, infoOf(id))} />
											{/each}
										{/if}
									</div>
								</div>
								<div class="flex shrink-0 flex-wrap items-center gap-2">
									<Button variant="secondary" onclick={() => (editing = role)}>{m.w6roles_section_edit()}</Button>
									<Confirm
										size="md"
										confirmLabel={m.w6roles_section_delete_confirm()}
										loading={removing === role.id}
										disabled={role.users > 0}
										onconfirm={() => remove(role)}
									>
										{m.w6roles_section_delete()}
									</Confirm>
								</div>
							</div>
							{#if role.users > 0}
								<p class="mt-2 text-[0.8125rem] text-ink-2">{m.w6roles_section_in_use()}</p>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}

			{#if editing === null}<p class="text-sm text-ink-2">{m.w6roles_section_admin_only()}</p>{/if}
			<DocLink page="using/settings/#custom-roles" />
		</div>
	{/if}
</Panel>
