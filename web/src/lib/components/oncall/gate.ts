/**
 * On-call schedules are an optional feature (Settings → System, `oncall_enabled`
 * on the instance settings). Turned off, nothing in the interface mentions
 * them: the Alerts tab, the on-call channel kind and the channels already made
 * with it all disappear. The data stays on the server, inactive.
 */

/** The channel kind that runs an escalation policy (`crate::oncall::CHANNEL_KIND`). */
export const ONCALL_CHANNEL_KIND = 'oncall';

/** Drops the on-call channel kind (or the channels of that kind) while the feature is off. */
export function withoutOnCall<T extends { kind: string }>(items: readonly T[], enabled: boolean): T[] {
	return enabled ? [...items] : items.filter((item) => item.kind !== ONCALL_CHANNEL_KIND);
}
