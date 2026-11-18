import type { BrowserContext } from '@playwright/test';
import { ADMIN, NAMES, expect, expectNoHorizontalOverflow, test } from './fixtures';

/**
 * The journeys a regression must not break, one after the other on one instance:
 * the steps are serial because each builds on the previous one (the account, then
 * the monitor, then the channel, then the outage). A failed step skips the rest.
 *
 * Time budget: the shortest probe interval the form offers is 30 s and the alert
 * engine evaluates every 10 s (set by the fixture); the built-in "Service down"
 * rule is shortened below so that a detected outage fires at once instead of
 * after its usual three minutes.
 */
test.describe.configure({ mode: 'serial' });

/** Browsers add this header themselves; a bare API client must send it (CSRF guard). */
const CSRF = { 'X-Requested-With': 'dumbmonit' };

/** Id of the monitor created by step 2, read by the steps that follow. */
let monitorId = 0;

interface ApiRule {
	id: number;
	uid: string;
	builtin?: boolean;
	overrides?: unknown;
	for_secs: number;
	[key: string]: unknown;
}

/** Engine phase of the "Service down" alert of the monitor: `null` when there is none. */
async function serviceDownPhase(context: BrowserContext): Promise<string | null> {
	const response = await context.request.get('/api/alerts', { headers: CSRF });
	expect(response.ok(), 'GET /api/alerts').toBeTruthy();
	const alerts = (await response.json()) as { rule_uid: string; target_id: number | null; effective_phase: string }[];
	return alerts.find((a) => a.rule_uid === 'service_down' && a.target_id === monitorId)?.effective_phase ?? null;
}

test('1. first run: the setup creates the admin account and opens the wizard', async ({ page, context, env }) => {
	await page.goto('/');
	// A fresh instance has no account: every address leads to the setup.
	await expect(page).toHaveURL(/\/setup$/);
	await expect(page.getByRole('heading', { level: 1 })).toContainText(/create your account/i);
	await expectNoHorizontalOverflow(page);

	await page.getByLabel('Setup code', { exact: true }).fill(env.setupCode);
	await page.getByLabel('Username', { exact: true }).fill(ADMIN.username);
	await page.getByLabel('Password', { exact: true }).fill(ADMIN.password);
	await page.getByLabel('Confirm password', { exact: true }).fill(ADMIN.password);
	await page.getByRole('button', { name: 'Create account and continue' }).click();

	// Signed in as soon as the account exists, and the setup wizard takes over.
	await expect(page).toHaveURL(/\/welcome/);
	await expect(page.getByRole('heading', { level: 1 })).toContainText('Your first alert');
	await expectNoHorizontalOverflow(page);

	// The step lives in the URL: the next one is "Name this instance".
	await page.goto('/welcome?step=instance');
	await expect(page.getByRole('heading', { level: 1 })).toContainText('Name this instance');
	await expectNoHorizontalOverflow(page);

	await page.getByRole('button', { name: 'Skip setup', exact: true }).click();
	await expect.poll(() => new URL(page.url()).pathname).toBe('/');
	await expect(page.locator('main#main')).toBeVisible();
	await expect(page).toHaveTitle(/DumbMonit/);
	await expectNoHorizontalOverflow(page);

	// The next steps start from this session.
	env.storageState = await context.storageState();
});

test('2. add an HTTP monitor: it goes up', async ({ page, env }) => {
	await page.goto('/targets');
	await page.getByRole('link', { name: 'Add a monitor' }).first().click();
	await expect(page).toHaveURL(/\/targets\/new/);
	await expectNoHorizontalOverflow(page);

	await page.getByRole('searchbox', { name: 'Search what to monitor' }).fill('Website or web API');
	await page.locator('[data-kind="http"]').first().click();
	await expect(page).toHaveURL(/kind=http/);

	await page.getByLabel('Name', { exact: true }).fill(NAMES.monitor);
	await page.getByLabel('Address', { exact: true }).fill(env.targetUrl());
	// 30 s is the shortest interval the form offers.
	await page.getByRole('button', { name: 'More options' }).click();
	await page.getByLabel('Check interval').selectOption({ label: '30 s' });
	await expectNoHorizontalOverflow(page);
	await page.getByRole('button', { name: 'Add monitor', exact: true }).click();

	await expect(page).toHaveURL(/\/targets\/\d+$/);
	monitorId = Number(new URL(page.url()).pathname.split('/').pop());
	await expect(page.getByRole('heading', { level: 1, name: NAMES.monitor })).toBeVisible();
	// "Reporting" is the word the UI gives a service whose last probe succeeded.
	const header = page.locator('section[aria-labelledby="device-name"]');
	await expect(header).toContainText('Reporting', { timeout: 90_000 });
	await expectNoHorizontalOverflow(page);
	expect(env.target.hits, 'the monitor reached the test service').toBeGreaterThan(0);
});

test('3. notification channel and public status page, both healthy', async ({ page, env, visitor }) => {
	// --- A webhook channel towards the test receiver --------------------------
	await page.goto('/settings/notifications');
	await page.getByRole('button', { name: 'Add channel' }).first().click();
	const form = page.getByRole('region', { name: 'Add channel' });
	await form.getByRole('searchbox', { name: 'Search channel types' }).fill('webhook');
	await form.getByRole('button', { name: /^Custom webhook/ }).click();
	await form.getByLabel('Name', { exact: true }).fill(NAMES.channel);
	await form.getByLabel('Address to call').fill(env.hookUrl());
	await expectNoHorizontalOverflow(page);
	await form.getByRole('button', { name: 'Add channel' }).click();
	await expect(page.getByText(NAMES.channel).first()).toBeVisible();

	// A test message proves the container reaches the receiver at all.
	await page.getByRole('button', { name: 'Send test' }).first().click();
	await env.hook.waitFor(() => true, 30_000);
	await expect(page.getByText('Test message sent').first()).toBeVisible();

	// --- A published status page with the monitor ------------------------------
	await page.goto('/status/new');
	await page.getByLabel('Title', { exact: true }).fill(NAMES.statusTitle);
	await page.getByLabel('Address', { exact: true }).fill(NAMES.statusSlug);
	await page.getByRole('switch', { name: 'Published' }).click();
	await page.getByRole('combobox', { name: 'Search a service' }).click();
	await page.getByRole('option', { name: NAMES.monitor }).click();
	await expectNoHorizontalOverflow(page);
	await page.getByRole('button', { name: 'Create page', exact: true }).click();
	await expect(page).toHaveURL(/\/status(\?|$)/);
	await expect(page.getByText(`/s/${NAMES.statusSlug}`).first()).toBeVisible();

	// --- A visitor with no account sees it, and everything is fine ------------
	await visitor.goto(`/s/${NAMES.statusSlug}`);
	await expect(visitor.getByRole('heading', { level: 1 })).toContainText(NAMES.statusTitle);
	await expect(visitor.getByText(NAMES.monitor).first()).toBeVisible();
	await expect(visitor.getByRole('region', { name: 'Current status' })).toContainText('All systems operational');
	await expectNoHorizontalOverflow(visitor);
});

test('4. outage: the alert fires, the webhook is called, the public page shows it', async ({ page, context, env, visitor }) => {
	test.setTimeout(300_000);

	// The built-in rule waits three minutes before firing: hold it for no time.
	const rules = (await (await context.request.get('/api/alerts/rules', { headers: CSRF })).json()) as ApiRule[];
	const rule = rules.find((r) => r.uid === 'service_down');
	expect(rule, 'the built-in "Service down" rule').toBeTruthy();
	const { id, builtin: _builtin, overrides: _overrides, ...payload } = rule!;
	const updated = await context.request.put(`/api/alerts/rules/${id}`, {
		headers: CSRF,
		data: { ...payload, for_secs: 0 }
	});
	expect(updated.ok(), `PUT /api/alerts/rules/${id}`).toBeTruthy();

	// The service dies: connection refused on its port.
	await env.target.stop();

	// The engine says so ...
	await expect.poll(() => serviceDownPhase(context), { timeout: 150_000, intervals: [3_000] }).toBe('firing');

	// ... the Alerts page shows it ...
	await page.goto('/alerts');
	await expect(page.getByRole('main')).toContainText('Service down', { timeout: 60_000 });
	await expect(page.getByRole('main')).toContainText(NAMES.monitor);
	await expectNoHorizontalOverflow(page);

	// ... the webhook was called for this monitor ...
	const firing = await env.hook.waitFor(
		(r) => r.json?.status === 'firing' && r.body.includes(NAMES.monitor),
		60_000
	);
	expect(firing.method).toBe('POST');

	// ... and the public page tells the visitor (its document is cached for up to 30 s).
	await visitor.goto(`/s/${NAMES.statusSlug}`);
	await expect(async () => {
		await visitor.reload();
		await expect(visitor.getByRole('region', { name: 'Current status' })).toContainText('Major outage', { timeout: 5_000 });
	}).toPass({ timeout: 120_000, intervals: [5_000] });
	await expect(visitor.getByText('Down', { exact: true }).first()).toBeVisible();
	await expectNoHorizontalOverflow(visitor);
});

test('5. recovery: the alert resolves, the webhook says so, the public page recovers', async ({ context, env, visitor }) => {
	test.setTimeout(300_000);

	await env.target.start();

	await expect.poll(() => serviceDownPhase(context), { timeout: 150_000, intervals: [3_000] }).not.toBe('firing');
	const resolved = await env.hook.waitFor(
		(r) => r.json?.status === 'resolved' && r.body.includes(NAMES.monitor),
		60_000
	);
	expect(resolved.method).toBe('POST');

	// A service that failed within the hour reads "Degraded" rather than "Operational"
	// (intermittent failure): what matters is that the outage is over.
	await visitor.goto(`/s/${NAMES.statusSlug}`);
	await expect(async () => {
		await visitor.reload();
		const banner = visitor.getByRole('region', { name: 'Current status' });
		await expect(banner).not.toContainText('Major outage', { timeout: 5_000 });
		await expect(banner).toContainText(/Partial outage|Degraded|All systems operational/, { timeout: 5_000 });
	}).toPass({ timeout: 150_000, intervals: [5_000] });
	await expectNoHorizontalOverflow(visitor);
});

test('6. the wall displays with no console error', async ({ page }) => {
	await page.goto('/wall');
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
	await expect(page.getByRole('button', { name: 'Exit wall mode' })).toBeVisible();
	// Let its scenes, widgets and polling run: errors that come late count too.
	await page.waitForTimeout(5_000);
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expectNoHorizontalOverflow(page);

	await page.getByRole('button', { name: 'Exit wall mode' }).click();
	await expect.poll(() => new URL(page.url()).pathname).not.toBe('/wall');
});

test('7. sign out, then sign in again', async ({ page }) => {
	await page.goto('/');
	await page.getByRole('button', { name: /Account menu for/ }).first().click();
	await page.getByRole('menuitem', { name: 'Sign out' }).click();
	await expect(page).toHaveURL(/\/login/);
	await expectNoHorizontalOverflow(page);

	// A wrong password is refused with a clear message ...
	await page.getByLabel('Username', { exact: true }).fill(ADMIN.username);
	await page.getByLabel('Password', { exact: true }).fill('not the right password');
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect(page.getByText('Wrong username or password.')).toBeVisible();

	// ... the right one opens the app.
	await page.getByLabel('Password', { exact: true }).fill(ADMIN.password);
	await page.getByRole('button', { name: 'Sign in', exact: true }).click();
	await expect.poll(() => new URL(page.url()).pathname).toBe('/');
	await expect(page.locator('main#main')).toBeVisible();
});
