import { request } from './client';
import type {
	Channel,
	ChannelKindInfo,
	ChannelMatcher,
	ChannelPayload,
	ChannelTestReport,
	MatchPreview,
	NotificationPolicy,
	NotificationPolicyPayload,
	RuleOverride,
	RuleOverridePayload,
	TargetId,
	TargetRules
} from './types';

// --- Notification channels --------------------------------------------------

export function listChannelKinds(signal?: AbortSignal): Promise<ChannelKindInfo[]> {
	return request<ChannelKindInfo[]>('/notify/kinds', { signal });
}

export function listChannels(signal?: AbortSignal): Promise<Channel[]> {
	return request<Channel[]>('/notify/channels', { signal });
}

export function createChannel(payload: ChannelPayload): Promise<Channel> {
	return request<Channel>('/notify/channels', { method: 'POST', body: payload });
}

export function updateChannel(id: number, payload: ChannelPayload): Promise<Channel> {
	return request<Channel>(`/notify/channels/${id}`, { method: 'PUT', body: payload });
}

export function deleteChannel(id: number): Promise<void> {
	return request<void>(`/notify/channels/${id}`, { method: 'DELETE' });
}

/** Sends a test message. Can take a few seconds: the caller shows an indicator. */
export function testChannel(id: number): Promise<ChannelTestReport> {
	return request<ChannelTestReport>(`/notify/channels/${id}/test`, { method: 'POST' });
}

// --- Notification policy and per-device overrides --------------------------

export function getNotificationPolicy(signal?: AbortSignal): Promise<NotificationPolicy> {
	return request<NotificationPolicy>('/notify/policy', { signal });
}

export function updateNotificationPolicy(payload: NotificationPolicyPayload): Promise<NotificationPolicy> {
	return request<NotificationPolicy>('/notify/policy', { method: 'PUT', body: payload });
}

/**
 * Which of the current devices a routing filter selects.
 *
 * The server runs the very filter the alerting engine runs, so the preview and
 * the delivered notifications can never disagree.
 */
export function previewMatcher(matcher: ChannelMatcher, signal?: AbortSignal): Promise<MatchPreview> {
	return request<MatchPreview>('/notify/match-preview', {
		method: 'POST',
		body: { matcher },
		signal
	});
}

/** Overrides of every rule, optionally only those of one device. */
export function listRuleOverrides(targetId?: TargetId, signal?: AbortSignal): Promise<RuleOverride[]> {
	const query = targetId === undefined ? '' : `?target_id=${targetId}`;
	return request<RuleOverride[]>(`/alerts/overrides${query}`, { signal });
}

/** Sets a rule's override for a device. An empty payload removes it. */
export function putRuleOverride(
	ruleId: number,
	targetId: TargetId,
	payload: RuleOverridePayload
): Promise<RuleOverride> {
	return request<RuleOverride>(`/alerts/rules/${ruleId}/overrides/${targetId}`, {
		method: 'PUT',
		body: payload
	});
}

/** The alert rules that watch a device, from the metrics it has written. */
export function getTargetRules(targetId: TargetId, signal?: AbortSignal): Promise<TargetRules> {
	return request<TargetRules>(`/targets/${targetId}/rules`, { signal });
}

export function deleteRuleOverride(ruleId: number, targetId: TargetId): Promise<void> {
	return request<void>(`/alerts/rules/${ruleId}/overrides/${targetId}`, { method: 'DELETE' });
}
