/**
 * A rough, local password score for the setup and password-change forms.
 * Advice only: it never blocks anything the server accepts. Pure, no DOM.
 */

export type StrengthLevel = 0 | 1 | 2 | 3 | 4;

/** Very common passwords (lower case), checked as a whole and as a base word. */
const COMMON = new Set(
	(
		'password passw0rd p@ssw0rd p@ssword 123456 1234567 12345678 123456789 1234567890 12345 111111 000000 ' +
		'123123 654321 666666 121212 112233 abc123 abcd1234 qwerty qwertyuiop qwerty123 azerty azerty123 ' +
		'azertyuiop qazwsx 1q2w3e 1q2w3e4r 1qaz2wsx zaq12wsx iloveyou admin admin123 administrator root ' +
		'toor letmein welcome welcome1 welcome123 login master monkey dragon football baseball soccer ' +
		'hockey batman superman trustno1 sunshine princess shadow michael jordan harley ranger hunter ' +
		'buster thomas robert soccer killer pepper tigger summer winter spring autumn secret changeme ' +
		'default guest user test test123 testing temp temp123 demo demo123 passpass access flower ' +
		'hello hello123 freedom whatever starwars pokemon cheese computer internet server monitor ' +
		'homelab dumbmonit proxmox synology docker linux ubuntu debian raspberry nopassword motdepasse ' +
		'bonjour soleil doudou marseille chocolat'
	).split(/\s+/)
);

/** Digits and symbols that stand for letters: `p@ssw0rd` reads as `password`. */
function unleet(value: string): string {
	return value.replace(/[@4]/g, 'a').replace(/0/g, 'o').replace(/[1!|]/g, 'i').replace(/3/g, 'e').replace(/[5$]/g, 's').replace(/7/g, 't');
}

/** True for a password on the common list, with or without a few trailing digits. */
export function isCommonPassword(value: string): boolean {
	const lower = value.toLowerCase();
	const bare = lower.replace(/[0-9!?.]+$/, '');
	return [lower, bare, unleet(lower), unleet(bare)].some((candidate) => candidate.length > 0 && COMMON.has(candidate));
}

/** One character repeated, or a run such as `abcdef` / `123456`. */
function isPattern(value: string): boolean {
	const chars = [...value];
	if (chars.length < 4) return false;
	if (new Set(chars).size === 1) return true;
	let steps = 0;
	for (let i = 1; i < chars.length; i += 1) {
		const diff = chars[i].codePointAt(0)! - chars[i - 1].codePointAt(0)!;
		if (diff === 1 || diff === -1) steps += 1;
	}
	return steps >= chars.length - 1;
}

/** 0 (empty or very weak) to 4 (strong). */
export function passwordStrength(value: string): StrengthLevel {
	const length = [...value].length;
	if (length === 0) return 0;
	if (isCommonPassword(value) || isPattern(value)) return 0;
	const classes = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^A-Za-z0-9]/].filter((re) => re.test(value)).length;
	const distinct = new Set([...value]).size;
	// A long phrase is worth as much as variety: points for length first.
	let points = 0;
	if (length >= 8) points += 1;
	if (length >= 12) points += 1;
	if (length >= 16) points += 1;
	if (length >= 22) points += 1;
	if (classes >= 3) points += 1;
	if (distinct < Math.min(length, 8) * 0.6) points -= 1;
	if (length < 8) points = Math.min(points, 1);
	return Math.max(0, Math.min(4, points)) as StrengthLevel;
}
