<script lang="ts">
	/** The reset link just created for an account: sent, or shown once to copy. */
	import type { User } from '#lib/api/index.js';
	import type { AccountLink } from '#lib/api/accounts.js';
	import { formatDateTime } from '#lib/format.js';
	import { Button, CopyBlock, Plate } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		user: User;
		link: AccountLink;
		ondone: () => void;
	}

	let { user, link, ondone }: Props = $props();
</script>

<div class="rise-in mb-4 rounded-[var(--radius-card)] border border-advisory/40 bg-surface p-4">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<div class="flex min-w-0 flex-wrap items-center gap-2">
			<p class="font-semibold break-all text-ink">
				{link.emailed && user.email
					? m.w6accounts_resetlink_sent({ username: user.username, email: user.email })
					: m.w6accounts_resetlink_copy({ username: user.username })}
			</p>
			<Plate tone="advisory" label={m.w6accounts_link_shown()} />
		</div>
		<Button variant="ghost" size="sm" onclick={ondone}>{m.settings_users_done()}</Button>
	</div>
	<p class="mt-1 text-[0.8125rem] text-ink-2">{m.w6accounts_resetlink_expires({ until: formatDateTime(link.expires_at) })}</p>
	{#if link.email_error}
		<p class="mt-1 text-[0.8125rem] text-warning-ink">{m.w6accounts_invites_email_failed({ error: link.email_error })}</p>
	{/if}
	<div class="mt-3">
		<CopyBlock value={link.url} label={m.w6accounts_invites_copy_link()} secret />
	</div>
</div>
