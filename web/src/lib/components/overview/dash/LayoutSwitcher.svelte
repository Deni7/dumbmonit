<script lang="ts">
	/**
	 * Which Overview this browser shows: Classic, Command, NOC or Hybrid. A
	 * compact segmented control (native radios: one tab stop, arrows move and
	 * choose) sitting in the header row, 32px high so the figures under it do
	 * not move; each segment's hit area still covers 40px. The choice is kept
	 * per browser (`layoutChoice`).
	 */
	import type { OverviewLayout } from '#lib/api/index.js';
	import { m } from '#lib/paraglide/messages.js';
	import { LAYOUTS, layoutChoice } from './widgets.svelte.js';

	function label(layout: OverviewLayout): string {
		switch (layout) {
			case 'classic':
				return m.w9layout_switch_classic();
			case 'command':
				return m.w9layout_switch_command();
			case 'noc':
				return m.w9layout_switch_noc();
			case 'hybrid':
				return m.w9layout_switch_hybrid();
		}
	}
</script>

<fieldset class="m-0 min-w-0 border-0 p-0">
	<legend class="sr-only">{m.w9layout_switch_legend()}</legend>
	<div class="flex h-8 w-fit max-w-full items-center gap-0.5 rounded-lg border border-line bg-surface p-0.5">
		{#each LAYOUTS as layout (layout)}
			<label
				class={`relative inline-flex h-[26px] shrink-0 cursor-pointer items-center rounded-md px-2.5 text-xs whitespace-nowrap transition-colors before:absolute before:inset-x-0 before:-inset-y-[7px] before:content-[''] has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-focus ${layoutChoice.value === layout ? 'bg-surface-2 font-semibold text-ink shadow-lift' : 'font-medium text-ink-2 hover:text-ink'}`}
			>
				<input
					type="radio"
					class="sr-only"
					name="overview-layout"
					value={layout}
					checked={layoutChoice.value === layout}
					onchange={() => layoutChoice.set(layout)}
				/>
				{label(layout)}
			</label>
		{/each}
	</div>
</fieldset>
