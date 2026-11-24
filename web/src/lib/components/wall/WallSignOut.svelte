<script lang="ts">
	/**
	 * Sign out, in place of Exit, on a wall-display account (`wall` role): the
	 * wall is the only page it may open, so leaving it means signing out. Two
	 * steps, so a display nobody watches does not sign out on a stray tap.
	 */
	import { LogOut } from 'lucide-svelte';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Confirm } from '#lib/ui/index.js';

	let signingOut = $state(false);

	async function signOut() {
		signingOut = true;
		try {
			await auth.logout();
		} finally {
			signingOut = false;
		}
	}
</script>

<span title={m.wall_sign_out_title()}>
	<Confirm variant="secondary" confirmLabel={m.wall_sign_out_confirm()} loading={signingOut} onconfirm={signOut} class="min-h-10">
		<LogOut class="size-4" aria-hidden="true" />
		{m.wall_sign_out()}
	</Confirm>
</span>
