<script lang="ts">
	/**
	 * No monitor yet: what to add first. Administrators get the primary button
	 * and the two other ways in (an agent, an agent behind a relay).
	 */
	import { withBase } from '#lib/paths.js';
	import { m } from '#lib/paraglide/messages.js';
	import { auth } from '#lib/stores/auth.svelte.js';
	import { Button, ClickSpark, EmptyState } from '#lib/ui/index.js';
</script>

<EmptyState
	mascot="watch"
	title={m.devices_targets_empty_title()}
	description={auth.isAdmin ? m.devices_targets_empty_admin() : m.devices_targets_empty_viewer()}
>
	{#snippet action()}
		{#if auth.isAdmin}
			{@const linkClass = 'font-semibold text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink'}
			<ClickSpark>
				<Button variant="primary" href="/targets/new">{m.devices_targets_add()}</Button>
			</ClickSpark>
			<p class="mt-4 text-sm text-ink-2">
				{#each m.devices_targets_empty_or({ agent: '\u0001', relay: '\u0002' }).split(/(\u0001|\u0002)/) as part, i (i)}
					{#if part === '\u0001'}
						<a href={withBase('/targets/new?kind=agent')} class={linkClass}>{m.devices_targets_empty_agent_link()}</a>
					{:else if part === '\u0002'}
						<a href={withBase('/targets/new?kind=agent&via=relay')} class={linkClass}>{m.devices_targets_empty_relay_link()}</a>
					{:else}{part}{/if}
				{/each}
			</p>
		{/if}
	{/snippet}
</EmptyState>
