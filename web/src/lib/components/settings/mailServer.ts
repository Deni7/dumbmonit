/**
 * Email server form (Settings → Notifications): what is sent to the server,
 * whether the stored password still applies, and what the status page editor
 * says about email subscriptions. Pure, so `npm test` can check it.
 */
import type { MailSecurity, MailServer, MailServerPayload } from '../../api/types';

export interface MailServerForm {
	host: string;
	/** As typed; empty: the usual port of the encryption. */
	port: string;
	security: MailSecurity;
	username: string;
	/** Typed password; empty keeps the stored one. Never pre-filled. */
	password: string;
	/** Forget the stored password. */
	clearPassword: boolean;
	from_address: string;
	from_name: string;
}

export function defaultPort(security: MailSecurity): number {
	return security === 'tls' ? 465 : security === 'none' ? 25 : 587;
}

export function formFrom(server: MailServer): MailServerForm {
	return {
		host: server.host,
		port: server.port === null ? '' : String(server.port),
		security: server.security,
		username: server.username,
		password: '',
		clearPassword: false,
		from_address: server.from_address,
		from_name: server.from_name
	};
}

/** The typed port, or `null` (usual port) when empty; `undefined` when it is not a port. */
export function parsePort(raw: string): number | null | undefined {
	const text = raw.trim();
	if (text === '') return null;
	if (!/^\d{1,5}$/.test(text)) return undefined;
	const port = Number(text);
	return port >= 1 && port <= 65535 ? port : undefined;
}

/**
 * The `PUT` body. Secrets follow the channel rule: omitted keeps the stored
 * password, `{}` clears it, a typed one replaces it.
 */
export function mailServerPayload(form: MailServerForm): MailServerPayload {
	const payload: MailServerPayload = {
		host: form.host.trim(),
		port: parsePort(form.port) ?? null,
		security: form.security,
		username: form.username.trim(),
		from_address: form.from_address.trim(),
		from_name: form.from_name.trim()
	};
	if (form.password !== '') payload.secrets = { password: form.password };
	else if (form.clearPassword) payload.secrets = {};
	return payload;
}

/** The stored password is kept only for the same host and the same username (the server says so too). */
export function storedPasswordApplies(server: MailServer | null, form: MailServerForm): boolean {
	return (
		!!server &&
		!server.locked_by_env &&
		server.has_password &&
		!form.clearPassword &&
		server.host.toLowerCase() === form.host.trim().toLowerCase() &&
		server.username === form.username.trim()
	);
}

/**
 * What the status page editor says under "Let visitors subscribe by email":
 * off; sent through a chosen channel; through the email server; no server set
 * yet (the page offers nothing until one is); or not known (not readable).
 */
export type EmailSubscription = 'off' | 'channel' | 'server' | 'missing' | 'unknown';

export function emailSubscription(open: boolean, channelId: number | null, server: Pick<MailServer, 'configured'> | null): EmailSubscription {
	if (!open) return 'off';
	if (channelId !== null) return 'channel';
	if (server === null) return 'unknown';
	return server.configured ? 'server' : 'missing';
}
