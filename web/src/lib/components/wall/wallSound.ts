/**
 * The short beep for a new critical problem, generated with WebAudio (no
 * asset). Browsers keep audio locked until a gesture: `unlockWallSound` is
 * called from one, and until then `beep` does nothing rather than failing.
 */
let context: AudioContext | null = null;

/** Creates or resumes the audio context; call from a click, tap or key press. */
export function unlockWallSound(): void {
	try {
		if (!context) {
			const Ctor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
			if (!Ctor) return;
			context = new Ctor();
		}
		if (context.state === 'suspended') void context.resume().catch(() => {});
	} catch {
		context = null;
	}
}

/** Whether a beep would be heard now. */
export function wallSoundReady(): boolean {
	return context !== null && context.state === 'running';
}

/** Two short tones; false when the sound is still locked or unsupported. */
export function beep(): boolean {
	const ctx = context;
	if (!ctx || ctx.state !== 'running') return false;
	try {
		const start = ctx.currentTime;
		for (const [i, frequency] of [880, 660].entries()) {
			const osc = ctx.createOscillator();
			const gain = ctx.createGain();
			const at = start + i * 0.22;
			osc.type = 'sine';
			osc.frequency.value = frequency;
			gain.gain.setValueAtTime(0.0001, at);
			gain.gain.exponentialRampToValueAtTime(0.25, at + 0.02);
			gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.18);
			osc.connect(gain).connect(ctx.destination);
			osc.start(at);
			osc.stop(at + 0.2);
		}
		return true;
	} catch {
		return false;
	}
}
