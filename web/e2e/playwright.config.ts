import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end journeys against the real image (see `docs/development.md`).
 *
 * The container is not started here but by the worker fixture in `fixtures.ts`:
 * one fresh container, on an empty volume, per project and per attempt. That is
 * why the suite is serial (`workers: 1`) and why `desktop` and `mobile` are two
 * independent runs (`--project=desktop`, `--project=mobile`), not two tabs of
 * the same instance: the first-run setup can only happen once per volume.
 */
export default defineConfig({
	testDir: '.',
	testMatch: '*.spec.ts',
	// Screenshots, traces and the container log of a failed run: what CI uploads.
	outputDir: '../test-results',
	fullyParallel: false,
	workers: 1,
	// A retry would restart the whole serial journey on a new container: no
	// hidden flakiness, a red run is a red run.
	retries: 0,
	forbidOnly: !!process.env.CI,
	timeout: 60_000,
	expect: { timeout: 10_000 },
	reporter: process.env.CI
		? [['list'], ['github'], ['html', { open: 'never', outputFolder: '../playwright-report' }]]
		: [['list'], ['html', { open: 'never', outputFolder: '../playwright-report' }]],
	use: {
		// The web port only serves HTTPS, with a self-signed certificate.
		ignoreHTTPSErrors: true,
		locale: 'en-US',
		timezoneId: 'UTC',
		// Same final state straight away: no scrambled-text or fade animation to wait for.
		reducedMotion: 'reduce',
		trace: 'retain-on-failure',
		screenshot: 'only-on-failure',
		actionTimeout: 15_000,
		navigationTimeout: 30_000
	},
	projects: [
		{
			name: 'desktop',
			use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 } }
		},
		{
			// A phone 390 px wide, still Chromium (the only browser the job installs).
			name: 'mobile',
			use: {
				...devices['Desktop Chrome'],
				viewport: { width: 390, height: 844 },
				isMobile: true,
				hasTouch: true,
				deviceScaleFactor: 2
			}
		}
	]
});
