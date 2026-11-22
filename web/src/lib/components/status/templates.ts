/**
 * Canned messages for the incident forms: one per status, inserted into the
 * message field and edited before posting. Choosing one also proposes the
 * matching status.
 */
import { m } from '#lib/paraglide/messages.js';
import type { IncidentKind, IncidentStatus } from '#lib/api/index.js';

export interface MessageTemplate {
	status: IncidentStatus;
	label: () => string;
	text: () => string;
}

const INCIDENT: MessageTemplate[] = [
	{ status: 'investigating', label: () => m.status_words_st_investigating(), text: () => m.status_template_investigating() },
	{ status: 'identified', label: () => m.status_words_st_identified(), text: () => m.status_template_identified() },
	{ status: 'monitoring', label: () => m.status_words_st_monitoring(), text: () => m.status_template_monitoring() },
	{ status: 'resolved', label: () => m.status_words_st_resolved(), text: () => m.status_template_resolved() }
];

const MAINTENANCE: MessageTemplate[] = [
	{ status: 'scheduled', label: () => m.status_words_st_scheduled(), text: () => m.status_template_scheduled() },
	{ status: 'in_progress', label: () => m.status_words_st_in_progress(), text: () => m.status_template_in_progress() },
	{ status: 'completed', label: () => m.status_words_st_completed(), text: () => m.status_template_completed() }
];

export function templatesFor(kind: IncidentKind): MessageTemplate[] {
	return kind === 'maintenance' ? MAINTENANCE : INCIDENT;
}
