<script lang="ts">
	/**
	 * Shown instead of the app while the instance policy requires two-factor
	 * authentication from admins and operators and this session has none yet:
	 * the server refuses every other route (403 `totp_setup_required`). Once the
	 * second factor is on and the recovery codes are saved, the auth store
	 * refreshes and the layout shows the app again. Mounted by the root layout.
	 */
	import { LogOut, ShieldAlert } from 'lucide-svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import Logo from '#lib/components/Logo.svelte';
	import TwoFactorSection from '#lib/components/settings/TwoFactorSection.svelte';
	import { Button } from '#lib/ui/index.js';

	let signingOut = $state(false);

	async function signOut() {
		signingOut = true;
		// The layout guard sends the user back to the sign-in screen.
		await auth.logout();
	}
</script>

<main class="flex flex-1 flex-col items-center justify-center px-4 py-12">
	<section class="rise-in w-full max-w-xl rounded-[var(--radius-card)] border border-line bg-surface p-6 shadow-float sm:p-7" aria-labelledby="totp-gate-title">
		<div class="flex items-center gap-2.5">
			<Logo class="size-8" />
			<p class="m-0 text-[1.05rem] leading-tight font-bold tracking-tight text-ink">DumbMonit</p>
		</div>
		<h1 id="totp-gate-title" class="display mt-6 flex items-center gap-2 text-[1.5rem] text-ink sm:text-2xl">
			<ShieldAlert class="size-6 shrink-0 text-advisory" aria-hidden="true" />
			{m.auth_totp_gate_title()}
		</h1>
		<p class="mt-2 text-sm text-ink-2">
			{auth.user
				? m.auth_totp_gate_intro_user({ username: auth.user.username })
				: m.auth_totp_gate_intro()}
		</p>

		<TwoFactorSection required />

		<div class="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-line pt-5">
			<p class="text-[0.8125rem] text-ink-2">{m.auth_totp_gate_later()}</p>
			<Button variant="ghost" loading={signingOut} onclick={() => void signOut()}>
				<LogOut class="size-4" aria-hidden="true" />
				{m.auth_totp_gate_sign_out()}
			</Button>
		</div>
	</section>
</main>
