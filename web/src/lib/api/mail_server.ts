/**
 * Email server of the instance (Settings → Notifications → Email server): the
 * SMTP server that mails status page subscribers and the scheduled reports
 * that have no channel. Administrators only; the password never comes back.
 */
import { request } from './client';
import type { MailServer, MailServerPayload, MailServerTestReport } from './types';

export function getMailServer(signal?: AbortSignal): Promise<MailServer> {
	return request<MailServer>('/notify/mail-server', { signal });
}

export function saveMailServer(payload: MailServerPayload): Promise<MailServer> {
	return request<MailServer>('/notify/mail-server', { method: 'PUT', body: payload });
}

/** Sends a test email to `to`, with the unsaved form when given, else with the saved server. */
export function testMailServer(to: string, server?: MailServerPayload): Promise<MailServerTestReport> {
	return request<MailServerTestReport>('/notify/mail-server/test', { method: 'POST', body: { to, server } });
}
