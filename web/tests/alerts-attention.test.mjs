// Info alerts are not problems: nav badge and wall count only warnings and criticals. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { attentionCount, countAlertsByTarget, countsAsProblem, criticalCount, quietedOnWall, visibleOnWall } from '../src/lib/alerts-attention.ts';

const alert = (over) => ({ effective_phase: 'firing', acked: false, severity: 'warning', ...over });

test('warning and critical firing alerts count', () => {
	assert.equal(countsAsProblem(alert({})), true);
	assert.equal(countsAsProblem(alert({ severity: 'critical' })), true);
});

test('info alerts never count', () => {
	assert.equal(countsAsProblem(alert({ severity: 'info' })), false);
});

test('acknowledged or suppressed alerts do not count', () => {
	assert.equal(countsAsProblem(alert({ acked: true })), false);
	assert.equal(countsAsProblem(alert({ effective_phase: 'suppressed' })), false);
});

test('the wall total leaves info out', () => {
	assert.equal(attentionCount({ warnings: 0, advisories: 1, unreachable: 1, misconfigured: 0 }), 2);
});

test('per-monitor counts split firing, acknowledged and masked alerts', () => {
	const map = countAlertsByTarget([
		alert({ target_id: 1 }),
		alert({ target_id: 1, severity: 'critical' }),
		alert({ target_id: 1, acked: true }),
		alert({ target_id: 1, effective_phase: 'suppressed' }),
		alert({ target_id: 2, effective_phase: 'pending' }),
		alert({ target_id: 3, severity: 'info' }),
		alert({ target_id: null })
	]);
	assert.deepEqual(map.get(1), { firing: 2, acked: 1, suppressed: 1, total: 4 });
	assert.equal(map.has(2), false);
	assert.equal(map.has(3), false);
	assert.equal(map.size, 1);
});

test('the wall leaves out building-up, acknowledged and info alerts', () => {
	const kept = alert({ target_id: 1 });
	const shown = visibleOnWall([
		kept,
		alert({ effective_phase: 'pending' }),
		alert({ acked: true }),
		alert({ severity: 'info' }),
		alert({ effective_phase: 'suppressed' })
	]);
	assert.equal(shown.length, 2);
	assert.equal(shown[0], kept);
});

test('a monitor whose only alerts are building up or acknowledged is quieted on the wall', () => {
	const quieted = quietedOnWall([
		alert({ target_id: 1, effective_phase: 'pending' }),
		alert({ target_id: 2, acked: true }),
		alert({ target_id: 3, acked: true }),
		alert({ target_id: 3, severity: 'critical' }),
		alert({ target_id: 4, severity: 'info' }),
		alert({ target_id: null, acked: true })
	]);
	assert.deepEqual([...quieted].sort(), [1, 2]);
});

test('the top number counts the red problems only', () => {
	assert.equal(criticalCount([{ rank: 0 }, { rank: 0 }, { rank: 1 }, { rank: 2 }, { rank: 4 }, { rank: 6 }]), 2);
	assert.equal(criticalCount([{ rank: 1 }, { rank: 1 }]), 0);
});
