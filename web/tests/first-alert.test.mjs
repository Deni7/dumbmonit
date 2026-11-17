// "Your first alert": the wizard's first step and the first-run stopwatch. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	QUICK_CHANNEL_KINDS,
	expectedAlertMinutes,
	firstRunSteps,
	guessMonitor,
	ntfySubscribeUrl,
	ntfyTopic,
	quickKinds
} from '../src/lib/components/wizard/quickstart.ts';

const ALL = ['http', 'ping', 'tcp', 'snmp'];

test('one field: an URL is a website check, a host a ping, host:port a TCP check', () => {
	assert.deepEqual(guessMonitor(' https://nas.lan/health ', ALL), { name: 'nas.lan', address: 'https://nas.lan/health', kind: 'http' });
	assert.deepEqual(guessMonitor('HTTP://Example.org', ALL), { name: 'example.org', address: 'HTTP://Example.org', kind: 'http' });
	assert.deepEqual(guessMonitor('192.168.1.10', ALL), { name: '192.168.1.10', address: '192.168.1.10', kind: 'ping' });
	assert.deepEqual(guessMonitor('Router.lan', ALL), { name: 'router.lan', address: 'Router.lan', kind: 'ping' });
	assert.deepEqual(guessMonitor('fd00::5', ALL), { name: 'fd00::5', address: 'fd00::5', kind: 'ping' });
	assert.deepEqual(guessMonitor('nas.lan:22', ALL), { name: 'nas.lan', address: 'nas.lan:22', kind: 'tcp' });
	assert.deepEqual(guessMonitor('[fd00::5]:443', ALL), { name: 'fd00::5', address: '[fd00::5]:443', kind: 'tcp' });
});

test('one field: anything unclear goes to the full form', () => {
	assert.equal(guessMonitor('', ALL), null);
	assert.equal(guessMonitor('my nas', ALL), null);
	assert.equal(guessMonitor('ftp://nas.lan', ALL), null);
	assert.equal(guessMonitor('nas.lan:99999', ALL), null);
	assert.equal(guessMonitor('nas_lan!', ALL), null);
	// The kind is missing on this server.
	assert.equal(guessMonitor('https://nas.lan', ['ping']), null);
	assert.equal(guessMonitor('nas.lan', ['http']), null);
	assert.equal(guessMonitor('nas.lan:22', ['ping']), null);
});

test('quick channels: simplest first, only what the server lists', () => {
	assert.deepEqual(QUICK_CHANNEL_KINDS, ['ntfy', 'discord', 'telegram', 'smtp', 'webhook']);
	const catalogue = ['webhook', 'slack', 'smtp', 'ntfy', 'future_kind'].map((kind) => ({ kind }));
	assert.deepEqual(quickKinds(catalogue).map((k) => k.kind), ['ntfy', 'smtp', 'webhook']);
	assert.deepEqual(quickKinds([]), []);
});

test('ntfy: a long random topic and the page to subscribe to it', () => {
	const topic = ntfyTopic((bytes) => bytes.fill(0));
	assert.equal(topic, 'dumbmonit-aaaaaaaaaaaaaa');
	assert.match(ntfyTopic(), /^dumbmonit-[a-z2-9]{14}$/);
	assert.notEqual(ntfyTopic(), ntfyTopic());
	assert.equal(ntfySubscribeUrl(undefined, topic), `https://ntfy.sh/${topic}`);
	assert.equal(ntfySubscribeUrl('https://ntfy.home/', 'alerts'), 'https://ntfy.home/alerts');
	assert.equal(ntfySubscribeUrl('javascript:alert(1)', 'alerts'), 'https://ntfy.sh/alerts');
	assert.equal(ntfySubscribeUrl('https://ntfy.sh', ''), null);
});

test('expected delay: uptime checks under 4 minutes, the rest under 5', () => {
	assert.equal(expectedAlertMinutes('http'), 4);
	assert.equal(expectedAlertMinutes('ping'), 4);
	assert.equal(expectedAlertMinutes('snmp'), 5);
});

test('stopwatch: the steps passed, in order, timed from the first boot', () => {
	const parse = (value) => new Date(`${value.replace(' ', 'T')}Z`);
	const steps = firstRunSteps(
		{
			first_boot_at: '2026-11-16 10:00:00',
			setup_at: '2026-11-16 10:01:30',
			first_target_at: '2026-11-16 10:02:10',
			first_channel_at: null,
			first_notification_at: '2026-11-16 10:03:00',
			first_alert_at: null
		},
		parse
	);
	assert.deepEqual(
		steps.map((s) => [s.key, s.elapsed]),
		[
			['first_boot_at', 0],
			['setup_at', 90],
			['first_target_at', 130],
			['first_notification_at', 180]
		]
	);
	// No boot recorded: the account starts the clock.
	const late = firstRunSteps({ first_boot_at: null, setup_at: '2026-11-16 10:01:30', first_target_at: '2026-11-16 10:02:00', first_channel_at: null, first_notification_at: null, first_alert_at: null }, parse);
	assert.deepEqual(late.map((s) => s.elapsed), [0, 30]);
	assert.deepEqual(firstRunSteps(undefined, parse), []);
});
