// Before an install command: where a relay's gateway stands, and the fixes offered. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	channelReady,
	firewallCommands,
	gatewayListenLine,
	portOf,
	portOwnerCommands,
	relayStateOf
} from '../src/lib/components/device-form/agent-channel.ts';

const relay = (over = {}) => ({
	relay_id: 7,
	relay_name: 'relay-lyon',
	host: '192.168.10.5',
	port: 8443,
	listening: true,
	self_check: true,
	reachable: true,
	check_error: null,
	problem: null,
	...over
});

test('a gateway the relay tested at the site address is ready', () => {
	assert.equal(relayStateOf(relay()), 'ready');
	assert.equal(relayStateOf(relay({ reachable: null })), 'testing');
	assert.equal(relayStateOf(relay({ reachable: false, check_error: 'refused' })), 'unreachable');
	assert.equal(relayStateOf(relay({ listening: false, reachable: null })), 'waiting');
	// An older relay does not test itself: listening is all it can say.
	assert.equal(relayStateOf(relay({ self_check: false, reachable: null })), 'ready');
});

test('a known problem blocks the command instead of a silent wait', () => {
	for (const problem of ['closed', 'too_old', 'offline', 'bind_failed', 'loopback', 'no_tunnel']) {
		assert.equal(relayStateOf(relay({ problem })), 'blocked', problem);
	}
	// Even with an old successful test: an offline relay carries nothing.
	assert.equal(relayStateOf(relay({ problem: 'offline', reachable: true })), 'blocked');
});

test('the command shows only once the channel answers where it sends machines', () => {
	const direct = { enabled: true, url: 'https://monit:8443', host: 'monit', port: 8443, listen_port: 8443, relay: null };
	assert.equal(channelReady(null), false);
	assert.equal(channelReady({ ...direct, outcome: 'ok' }), true);
	assert.equal(channelReady({ ...direct, outcome: 'refused' }), false, 'port not published');
	assert.equal(channelReady({ ...direct, enabled: false, outcome: null }), true, 'channel off: the web port is used');
	assert.equal(channelReady({ ...direct, outcome: null, relay: relay() }), true);
	assert.equal(channelReady({ ...direct, outcome: null, relay: relay({ problem: 'closed' }) }), false);
	assert.equal(channelReady({ ...direct, outcome: null, relay: relay({ listening: false }) }), false);
});

test('ports are read from host:port and [v6]:port', () => {
	assert.equal(portOf('0.0.0.0:8443'), 8443);
	assert.equal(portOf('[::1]:9443'), 9443);
	assert.equal(portOf('relay'), null);
	assert.equal(portOf('x:70000'), null);
	assert.equal(portOf(null), null);
});

test('fix commands name the port for firewalld, ufw, Windows and the agent.yaml', () => {
	const firewall = firewallCommands(9443).split('\n');
	assert.equal(firewall.length, 3);
	assert.match(firewall[0], /firewall-cmd --add-port=9443\/tcp --permanent/);
	assert.match(firewall[1], /^sudo ufw allow 9443\/tcp$/);
	assert.match(firewall[2], /^New-NetFirewallRule .* -LocalPort 9443 -Action Allow$/);
	const owner = portOwnerCommands('8443');
	assert.match(owner, /ss -ltnp 'sport = :8443'/);
	assert.match(owner, /Get-NetTCPConnection -LocalPort 8443 -State Listen/);
	assert.equal(gatewayListenLine(8443), 'gateway_listen: 0.0.0.0:8443');
});
