<script lang="ts">
	/**
	 * Users list: "Send reset link" for an account that signs in with a local
	 * password. Emailed to its address when it has one and an email server is
	 * configured; the link comes back in every case, for `ResetLinkNotice`.
	 */
	import { KeyRound } from 'lucide-svelte';
	import type { User } from '#lib/api/index.js';
	import { createResetLink, type AccountLink } from '#lib/api/accounts.js';
	import { Button } from '#lib/ui/index.js';
	import { m } from '#lib/paraglide/messages.js';

	interface Props {
		user: User;
		disabled?: boolean;
		onlink: (user: User, link: AccountLink) => void;
		onerror: (cause: unknown) => void;
	}

	let { user, disabled = false, onlink, onerror }: Props = $props();
	let busy = $state(false);

	async function create() {
		busy = true;
		try {
			onlink(user, await createResetLink(user.id, !!user.email));
		} catch (cause) {
			onerror(cause);
		} finally {
			busy = false;
		}
	}
</script>

<span title={user.email ? m.w6accounts_resetlink_title_email({ email: user.email }) : m.w6accounts_resetlink_title_copy()}>
	<Button variant="ghost" size="sm" loading={busy} {disabled} onclick={() => void create()}>
		<KeyRound class="size-4" aria-hidden="true" />
		{m.w6accounts_resetlink_button()}
	</Button>
</span>
