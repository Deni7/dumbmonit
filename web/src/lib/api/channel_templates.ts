/**
 * Message templates of a notification channel: the server renders them on a
 * sample alert, with the escaping of the channel's service, and says why it
 * would refuse them (unknown variable, unclosed braces).
 */
import { request } from './client';
import type { TemplatePreview, TemplatePreviewRequest } from './types';

/** Settings keys of the templates (`channel.settings`); empty or absent = the default. */
export const MESSAGE_TITLE_TEMPLATE_KEY = 'message_title_template';
export const MESSAGE_BODY_TEMPLATE_KEY = 'message_body_template';
/** Settings key of the "Interactive buttons" switch (Slack, Telegram, Teams). */
export const INTERACTIVE_KEY = 'interactive';

export function previewMessageTemplate(payload: TemplatePreviewRequest, signal?: AbortSignal): Promise<TemplatePreview> {
	return request<TemplatePreview>('/notify/template-preview', { method: 'POST', body: payload, signal });
}
