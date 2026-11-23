/**
 * What the channel check before an install command decides, kept out of the
 * component so it can be tested: where a relay's gateway stands, and the
 * commands that fix the usual causes (firewall, a port held by another
 * program, a gateway bound to the loopback). See `AgentChannelCheck.svelte`.
 */
import type { AgentChannelCheck } from '#lib/api/types.js';

export type RelayCheck = NonNullable<AgentChannelCheck['relay']>;

/**
 * - `blocked`: the server knows why the gateway cannot accept the site (`problem`);
 * - `waiting`: asked, not listening yet (it opens within seconds);
 * - `testing`: listening, the relay has not tested it at this address yet;
 * - `unreachable`: the relay's own test at this address failed;
 * - `ready`: the relay's test passed (or an older relay says it listens).
 */
export type RelayState = 'blocked' | 'waiting' | 'testing' | 'unreachable' | 'ready';

export function relayStateOf(relay: RelayCheck): RelayState {
	if (relay.problem) return 'blocked';
	if (!relay.listening) return 'waiting';
	// An older relay does not test its gateway: listening is all it can say.
	if (!relay.self_check) return 'ready';
	if (relay.reachable === true) return 'ready';
	if (relay.reachable === false) return 'unreachable';
	return 'testing';
}

/** The install command shows once the channel answers where it sends machines. */
export function channelReady(result: AgentChannelCheck | null): boolean {
	if (!result) return false;
	if (!result.enabled) return true;
	if (result.relay) return relayStateOf(result.relay) === 'ready';
	return result.outcome === 'ok';
}

/** Port of a `host:port` or `[v6]:port` address; `null` when there is none. */
export function portOf(address: string | null | undefined): number | null {
	const match = /:(\d{1,5})$/.exec(address?.trim() ?? '');
	if (!match) return null;
	const port = Number(match[1]);
	return port >= 1 && port <= 65535 ? port : null;
}

/** Inbound TCP `port` on the host's firewall: firewalld, ufw, then Windows (PowerShell, as administrator). */
export function firewallCommands(port: number | string): string {
	return [
		`sudo firewall-cmd --add-port=${port}/tcp --permanent && sudo firewall-cmd --reload`,
		`sudo ufw allow ${port}/tcp`,
		`New-NetFirewallRule -DisplayName "DumbMonit agents" -Direction Inbound -Protocol TCP -LocalPort ${port} -Action Allow`
	].join('\n');
}

/** What holds `port` on the relay: Linux, then Windows (PowerShell). */
export function portOwnerCommands(port: number | string): string {
	return [
		`sudo ss -ltnp 'sport = :${port}'`,
		`Get-NetTCPConnection -LocalPort ${port} -State Listen | Select-Object LocalAddress, OwningProcess`
	].join('\n');
}

/** The agent.yaml line that opens the gateway on every interface of the relay. */
export function gatewayListenLine(port: number | string): string {
	return `gateway_listen: 0.0.0.0:${port}`;
}
