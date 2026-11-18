<script lang="ts">
	/**
	 * A monitor's state in a list: the LED and the state word, no plate. The
	 * word carries the state (never the LED's colour alone); only states that
	 * need a look take a tone, a healthy or disabled one stays quiet.
	 */
	import type { TargetState } from '#lib/format.js';
	import { STATE_LABEL, STATE_TONE } from '#lib/format.js';
	import { Led } from '#lib/ui/index.js';

	interface Props {
		state: TargetState;
		class?: string;
		title?: string;
	}

	let { state, class: className = '', title }: Props = $props();

	const tone = $derived(STATE_TONE[state]);
	const blink = $derived(state === 'offline' || state === 'down');
	const INK = { signal: 'text-ink-2', advisory: 'text-advisory-ink', warning: 'text-warning-ink', ghost: 'text-ink-3' } as const;
</script>

<span class={`inline-flex min-w-0 items-center gap-1.5 text-[0.8125rem] ${INK[tone]} ${className}`} {title}>
	<Led {tone} {blink} size="sm" />
	<span class={`truncate ${tone === 'warning' || tone === 'advisory' ? 'font-semibold' : ''}`}>{STATE_LABEL[state]}</span>
</span>
