import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import https from 'node:https';
import { dirname } from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);

async function docker(...args: string[]): Promise<string> {
	const { stdout } = await run('docker', args, { maxBuffer: 64 * 1024 * 1024 });
	return stdout.trim();
}

/** A DumbMonit instance the journeys talk to. */
export interface Instance {
	baseURL: string;
	/** Last lines of the server log (empty for an external instance). */
	logs(lines?: number): Promise<string>;
	/** Writes the whole server log to `path`, then removes the container and its volume. */
	stop(logPath: string): Promise<void>;
}

export interface StartOptions {
	image: string;
	/** Used in the container and volume names, to tell parallel runs apart. */
	label: string;
	setupCode: string;
	/** Host name the container uses to reach the test servers running on this machine. */
	hostName: string;
}

/** GET on the self-signed HTTPS port: true once it answers 200 with JSON. */
function healthy(url: string): Promise<boolean> {
	return new Promise((resolve) => {
		const request = https.get(url, { rejectUnauthorized: false, timeout: 3_000 }, (response) => {
			let body = '';
			response.setEncoding('utf8');
			response.on('data', (chunk) => (body += chunk));
			response.on('end', () => {
				try {
					JSON.parse(body);
					resolve(response.statusCode === 200);
				} catch {
					resolve(false);
				}
			});
		});
		request.on('timeout', () => request.destroy());
		request.on('error', () => resolve(false));
	});
}

/**
 * Starts the image on an empty volume, the way production runs it (non-root,
 * read-only root file system, no capabilities): what the upgrade test does too.
 * Everything the journeys need to be quick is an existing setting: the alert
 * engine evaluates every 10 s (its floor) instead of 30 s.
 */
export async function startInstance(options: StartOptions): Promise<Instance> {
	const suffix = randomBytes(3).toString('hex');
	const name = `dumbmonit-e2e-${options.label}-${suffix}`;
	const volume = `${name}-data`;

	await docker('volume', 'create', volume);
	await docker(
		'run',
		'-d',
		'--name',
		name,
		'--label',
		'dumbmonit-e2e=1',
		'-p',
		'127.0.0.1::8080',
		'-v',
		`${volume}:/data`,
		'-e',
		`DUMBMONIT_SETUP_CODE=${options.setupCode}`,
		'-e',
		'DUMBMONIT_ALERT_INTERVAL_SECS=10',
		'-e',
		'DUMBMONIT_UPDATE_CHECK=off',
		// The test servers listen on this machine; the container reaches them through this name.
		'--add-host',
		`${options.hostName}:host-gateway`,
		'--read-only',
		'--tmpfs',
		'/tmp',
		'--cap-drop',
		'ALL',
		'--security-opt',
		'no-new-privileges:true',
		'--sysctl',
		'net.ipv4.ping_group_range=0 2147483647',
		options.image
	);

	const logs = async (lines?: number) => {
		const args = ['logs'];
		if (lines) args.push('--tail', String(lines));
		args.push(name);
		// `docker logs` writes the container's stderr on its own stderr.
		const { stdout, stderr } = await run('docker', args, { maxBuffer: 64 * 1024 * 1024 });
		return `${stdout}${stderr}`.replace(/\x1b\[[0-9;]*m/g, '');
	};
	const stop = async (logPath: string) => {
		try {
			await mkdir(dirname(logPath), { recursive: true });
			await writeFile(logPath, await logs());
		} catch {
			// The log is a diagnostic: never hide the test result behind it.
		}
		await docker('rm', '-f', name).catch(() => {});
		await docker('volume', 'rm', '-f', volume).catch(() => {});
	};

	try {
		const mapping = (await docker('port', name, '8080/tcp')).split('\n')[0];
		const port = mapping.slice(mapping.lastIndexOf(':') + 1);
		const baseURL = `https://127.0.0.1:${port}`;
		for (let attempt = 0; attempt < 120; attempt++) {
			const state = await docker('inspect', '-f', '{{.State.Running}}', name);
			if (state !== 'true') throw new Error(`the container stopped at startup:\n${await logs(60)}`);
			if (await healthy(`${baseURL}/api/health`)) return { baseURL, logs, stop };
			await new Promise((resolve) => setTimeout(resolve, 1_000));
		}
		throw new Error(`no answer on /api/health after 120 s:\n${await logs(60)}`);
	} catch (error) {
		await stop(`${process.cwd()}/test-results/startup-failure-${name}.log`);
		throw error;
	}
}
