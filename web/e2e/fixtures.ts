import { test as base, expect, type BrowserContext, type ConsoleMessage, type Page } from '@playwright/test';
import { join } from 'node:path';
import { startInstance, type Instance } from './lib/container';
import { TargetServer, WebhookReceiver } from './lib/servers';

type StorageState = Awaited<ReturnType<BrowserContext['storageState']>>;

/** The first administrator, created by the first journey and reused by the others. */
export const ADMIN = { username: 'e2e-admin', password: 'correct horse battery 42' };

/** What the journeys create: one name each, shared by every step that looks for it. */
export const NAMES = {
	monitor: 'E2E web service',
	channel: 'E2E webhook',
	statusTitle: 'E2E status',
	statusSlug: 'e2e-status'
};

/**
 * Everything one serial journey shares: the instance under test, the two test
 * servers (the monitored service and the webhook receiver) and what the steps
 * learn along the way. Rebuilt with a fresh container whenever the worker restarts
 * (a failed step, a retry): a step never sees the leftovers of a previous run.
 */
export interface Env {
	baseURL: string;
	/** Name under which the container reaches this machine. */
	hostName: string;
	setupCode: string;
	instance: Instance;
	target: TargetServer;
	hook: WebhookReceiver;
	/** Session of the administrator, kept by the first journey for the next ones. */
	storageState?: StorageState;
	/** Address given to the HTTP monitor and to the webhook channel. */
	targetUrl(): string;
	hookUrl(): string;
}

/**
 * Console output that is part of normal operation. Everything else that DumbMonit
 * writes with `console.error`, and every uncaught exception, fails the test.
 * Keep this list short and each entry justified.
 */
function isExpected(message: ConsoleMessage): boolean {
	const text = message.text();
	const url = message.location().url;
	// Before signing in, the session probe is answered 401: the browser logs it by itself.
	if (/status of 401/.test(text) && /\/api\/auth\//.test(url)) return true;
	return false;
}

interface Guard {
	/** Starts watching a page opened outside the `page` fixture (the public visitor). */
	watch(page: Page): void;
}

interface Fixtures {
	guard: Guard;
	diagnostics: void;
	/** A browser context with no session, for the public pages. */
	visitor: Page;
}

export const test = base.extend<Fixtures, { env: Env }>({
	env: [
		async ({}, use, workerInfo) => {
			const hostName = process.env.E2E_HOST || 'host.docker.internal';
			const setupCode = process.env.E2E_SETUP_CODE || 'E2ETS-JOURN';
			const target = new TargetServer();
			const hook = new WebhookReceiver();
			await Promise.all([target.start(), hook.start()]);

			const external = process.env.E2E_BASE_URL;
			const instance: Instance = external
				? {
						baseURL: external.replace(/\/$/, ''),
						logs: async () => '',
						stop: async () => {}
					}
				: await startInstance({
						image: process.env.E2E_IMAGE || 'dumbmonit-candidate:ci',
						label: workerInfo.project.name,
						setupCode,
						hostName
					});
			const env: Env = {
				baseURL: instance.baseURL,
				hostName,
				setupCode,
				instance,
				target,
				hook,
				targetUrl: () => `http://${hostName}:${target.port}/`,
				hookUrl: () => `http://${hostName}:${hook.port}/hook`
			};
			try {
				await use(env);
			} finally {
				await instance.stop(join(workerInfo.project.outputDir, `container-${workerInfo.project.name}.log`));
				await Promise.all([target.stop(), hook.stop()]);
			}
		},
		{ scope: 'worker', timeout: 240_000 }
	],

	baseURL: async ({ env }, use) => {
		await use(env.baseURL);
	},
	// Each test opens a fresh context; once the first journey has signed in, it starts signed in.
	storageState: async ({ env }, use) => {
		await use(env.storageState);
	},

	// Fails the test on any unexpected console error or uncaught exception, on any page.
	guard: [
		async ({ context }, use) => {
			const problems: string[] = [];
			const watch = (page: Page) => {
				page.on('pageerror', (error) => problems.push(`pageerror: ${error.message}`));
				page.on('console', (message) => {
					if (message.type() === 'error' && !isExpected(message)) {
						problems.push(`console.error: ${message.text()} (${message.location().url})`);
					}
				});
			};
			context.pages().forEach(watch);
			context.on('page', watch);
			await use({ watch });
			expect(problems, 'unexpected console errors or uncaught exceptions').toEqual([]);
		},
		{ auto: true }
	],

	// A failed step also attaches the end of the server log to the report.
	diagnostics: [
		async ({ env }, use, testInfo) => {
			await use();
			if (testInfo.status !== testInfo.expectedStatus) {
				const log = await env.instance.logs(200).catch(() => '');
				if (log) await testInfo.attach('container-log-tail', { body: log, contentType: 'text/plain' });
			}
		},
		{ auto: true }
	],

	visitor: async (
		{ browser, baseURL, viewport, isMobile, hasTouch, deviceScaleFactor, locale, timezoneId, reducedMotion, ignoreHTTPSErrors, guard },
		use,
		testInfo
	) => {
		const context = await browser.newContext({
			baseURL,
			viewport,
			isMobile,
			hasTouch,
			deviceScaleFactor,
			locale,
			timezoneId,
			reducedMotion,
			ignoreHTTPSErrors
		});
		await context.tracing.start({ screenshots: true, snapshots: true });
		const page = await context.newPage();
		guard.watch(page);
		await use(page);
		if (testInfo.status !== testInfo.expectedStatus) {
			await page.screenshot({ path: testInfo.outputPath('visitor.png'), fullPage: true }).catch(() => {});
			await context.tracing.stop({ path: testInfo.outputPath('visitor-trace.zip') }).catch(() => {});
		} else {
			await context.tracing.stop().catch(() => {});
		}
		await context.close();
	}
});

export { expect };

/** No horizontal scroll: the rule at 390 px, checked on every screen of the journeys. */
export async function expectNoHorizontalOverflow(page: Page): Promise<void> {
	const overflow = await page.evaluate(
		() => document.documentElement.scrollWidth - document.documentElement.clientWidth
	);
	expect(overflow, `horizontal overflow on ${page.url()}`).toBeLessThanOrEqual(0);
}
