import http from 'node:http';
import type { AddressInfo, Socket } from 'node:net';

/** Listens on every interface: the container reaches it through the host gateway. */
const BIND = '0.0.0.0';

function listen(server: http.Server, port: number): Promise<number> {
	return new Promise((resolve, reject) => {
		server.once('error', reject);
		server.listen(port, BIND, () => {
			server.off('error', reject);
			resolve((server.address() as AddressInfo).port);
		});
	});
}

/**
 * The monitored service: answers 200 until `stop()` closes the port (connection
 * refused, as for a crashed service), and again after `start()` on the same port.
 */
export class TargetServer {
	port = 0;
	private server: http.Server | null = null;
	private sockets = new Set<Socket>();

	/** Requests answered while the service was up. */
	hits = 0;

	async start(): Promise<void> {
		if (this.server) return;
		const server = http.createServer((_request, response) => {
			this.hits++;
			response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8', connection: 'close' });
			response.end('dumbmonit e2e target: ok');
		});
		server.on('connection', (socket) => {
			this.sockets.add(socket);
			socket.on('close', () => this.sockets.delete(socket));
		});
		// The port is released as soon as `close()` returns, but be patient anyway.
		for (let attempt = 0; ; attempt++) {
			try {
				this.port = await listen(server, this.port);
				break;
			} catch (error) {
				if (attempt >= 20) throw error;
				await new Promise((resolve) => setTimeout(resolve, 250));
			}
		}
		this.server = server;
	}

	async stop(): Promise<void> {
		const server = this.server;
		if (!server) return;
		this.server = null;
		for (const socket of this.sockets) socket.destroy();
		await new Promise<void>((resolve) => server.close(() => resolve()));
	}

	get running(): boolean {
		return this.server !== null;
	}
}

/** One request received by the webhook receiver. */
export interface Received {
	method: string;
	url: string;
	body: string;
	/** The body parsed as JSON, or `null`. */
	json: Record<string, unknown> | null;
}

/** A notification endpoint that records what DumbMonit sends it. */
export class WebhookReceiver {
	port = 0;
	readonly received: Received[] = [];
	private server: http.Server | null = null;

	async start(): Promise<void> {
		const server = http.createServer((request, response) => {
			const chunks: Buffer[] = [];
			request.on('data', (chunk: Buffer) => chunks.push(chunk));
			request.on('end', () => {
				const body = Buffer.concat(chunks).toString('utf8');
				let json: Record<string, unknown> | null = null;
				try {
					const parsed = JSON.parse(body);
					if (parsed && typeof parsed === 'object') json = parsed as Record<string, unknown>;
				} catch {
					// Not JSON: kept as text.
				}
				this.received.push({ method: request.method ?? '', url: request.url ?? '', body, json });
				response.writeHead(200, { 'content-type': 'text/plain' });
				response.end('ok');
			});
		});
		this.port = await listen(server, 0);
		this.server = server;
	}

	async stop(): Promise<void> {
		const server = this.server;
		if (!server) return;
		this.server = null;
		server.closeAllConnections();
		await new Promise<void>((resolve) => server.close(() => resolve()));
	}

	/** Resolves with the first request (already received or not yet) that satisfies `match`. */
	async waitFor(match: (request: Received) => boolean, timeoutMs: number): Promise<Received> {
		const deadline = Date.now() + timeoutMs;
		for (;;) {
			const found = this.received.find(match);
			if (found) return found;
			if (Date.now() > deadline) {
				const seen = this.received.map((r) => r.body.slice(0, 300)).join('\n  ');
				throw new Error(`no matching webhook within ${timeoutMs} ms; received:\n  ${seen || '(nothing)'}`);
			}
			await new Promise((resolve) => setTimeout(resolve, 500));
		}
	}
}
