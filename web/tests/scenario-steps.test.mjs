// Steps of an HTTP scenario (`http_steps`) and its last run. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	emptyStep,
	lastRun,
	moveStep,
	parseSteps,
	serializeSteps,
	stepIssues,
	usedVariables,
	validVariableName
} from '../src/lib/components/device-form/scenario-steps.ts';

const step = (o = {}) => ({ ...emptyStep(), ...o });

test('serialising leaves defaults and blank lines out, and parses back', () => {
	const steps = [
		step({
			name: ' Login ',
			method: 'POST',
			url: '/login',
			headers: [
				{ name: 'Content-Type', value: 'application/json' },
				{ name: '', value: '' }
			],
			body: '{"p":"{{secret.password|json}}"}',
			assertions: [
				{ type: 'json', path: '$.ok', value: 'true' },
				{ type: 'contains', path: 'ignored', value: 'Welcome' },
				{ type: 'contains', path: '', value: '' }
			],
			extract: [
				{ name: 'token', from: 'json', expression: '$.token' },
				{ name: '', from: 'json', expression: '' }
			]
		}),
		step({ url: '/me' })
	];
	const raw = serializeSteps(steps);
	assert.deepEqual(JSON.parse(raw), [
		{
			name: 'Login',
			method: 'POST',
			url: '/login',
			headers: [{ name: 'Content-Type', value: 'application/json' }],
			body: '{"p":"{{secret.password|json}}"}',
			assertions: [
				{ type: 'json', path: '$.ok', value: 'true' },
				{ type: 'contains', value: 'Welcome' }
			],
			extract: [{ name: 'token', from: 'json', expression: '$.token' }]
		},
		{ url: '/me' }
	]);
	const back = parseSteps(raw);
	assert.equal(back.length, 2);
	assert.equal(back[1].method, 'GET');
	assert.equal(back[0].assertions[1].path, '');
	assert.equal(serializeSteps([]), '');
});

test('an unreadable tag gives no step', () => {
	assert.deepEqual(parseSteps(undefined), []);
	assert.deepEqual(parseSteps('not json'), []);
	assert.deepEqual(parseSteps('{"url":"/"}'), []);
	assert.equal(parseSteps('[{"url":"/","assertions":[{"type":"weird"}]}]')[0].assertions[0].type, 'contains');
});

test('steps move up and down, and stay put out of range', () => {
	assert.deepEqual(moveStep(['a', 'b', 'c'], 2, 0), ['c', 'a', 'b']);
	assert.deepEqual(moveStep(['a', 'b', 'c'], 0, 1), ['b', 'a', 'c']);
	const same = ['a', 'b'];
	assert.equal(moveStep(same, 1, 2), same);
});

test('variables are found, secrets and filters aside', () => {
	assert.deepEqual(usedVariables('/x/{{ id }}/{{token|url}}/{{secret.password}}/{{id}}'), ['id', 'token']);
	assert.ok(validVariableName('order_id'));
	assert.ok(!validVariableName('2x'));
	assert.ok(!validVariableName('secret'));
});

test('a variable must be extracted by an earlier step', () => {
	const issues = stepIssues([
		step({ url: '/a/{{token}}' }),
		step({ url: '/login', extract: [{ name: 'token', from: 'header', expression: 'x-token' }] }),
		step({ url: '', headers: [{ name: 'Authorization', value: 'Bearer {{token}}' }] }),
		step({ url: '/x', extract: [{ name: '9bad', from: 'json', expression: '$.a' }] })
	]);
	assert.deepEqual(issues.get(0), [{ kind: 'unknown_variable', name: 'token' }]);
	assert.equal(issues.has(1), false);
	assert.deepEqual(issues.get(2), [{ kind: 'url_missing' }]);
	assert.deepEqual(issues.get(3), [{ kind: 'bad_name', name: '9bad' }]);
});

test('the last run keeps the newest run only, and statuses from that run', () => {
	const results = [
		{ step: 1, value: 1, at: 1000 },
		{ step: 2, value: 0, at: 1000 },
		{ step: 3, value: 2, at: 1000 },
		// A fourth step from an older version of the scenario.
		{ step: 4, value: 1, at: 400 }
	];
	const statuses = [
		{ step: 1, value: 200, at: 1000 },
		// Step 2 failed before any answer this time: its 200 is from an older run.
		{ step: 2, value: 200, at: 700 }
	];
	const durations = [
		{ step: 1, value: 0.12, at: 0 },
		{ step: 2, value: 5, at: 0 },
		{ step: 3, value: 0.3, at: 0 }
	];
	const runs = lastRun(results, statuses, durations, [
		{ name: 'Login', method: 'POST', url: '/login' },
		{ name: '', method: 'GET', url: '/me' },
		{ name: '', method: 'GET', url: '' }
	]);
	assert.deepEqual(runs, [
		{ index: 1, title: 'Login', result: 'passed', status: 200, seconds: 0.12 },
		{ index: 2, title: 'GET /me', result: 'failed', status: null, seconds: 5 },
		{ index: 3, title: '', result: 'skipped', status: null, seconds: null }
	]);
	assert.deepEqual(lastRun([], [], [], []), []);
});
