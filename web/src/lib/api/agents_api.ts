import { request, ApiError } from './client';
import type {
	AdKitPayload,
	AgentCommandView,
	AgentHost,
	AgentModule,
	AgentToken,
	AgentTokenPayload,
	CreatedAdKit,
	CreatedAgentToken,
	RebindWindow,
	TargetId,
	TokenMachine
} from './types';

// --- Agents -----------------------------------------------------------------

export function listAgentTokens(signal?: AbortSignal): Promise<AgentToken[]> {
	return request<AgentToken[]>('/agent/tokens', { signal });
}

/**
 * Creates an enrolment token. `baseUrl` is the address through which machines
 * will reach this server — in practice `location.origin`.
 *
 * Without a scope the token is single use: it enrols one machine and then
 * enrols nothing more. Pass `reusable` for a fleet.
 */
export function createAgentToken(payload: AgentTokenPayload): Promise<CreatedAgentToken> {
	return request<CreatedAgentToken>('/agent/tokens', { method: 'POST', body: payload });
}

/** Machines enrolled with this token, newest first: the add flow waits for the first one. */
export function listTokenMachines(id: number, signal?: AbortSignal): Promise<TokenMachine[]> {
	return request<TokenMachine[]>(`/agent/tokens/${id}/machines`, { signal });
}

/** Creates a fleet token and the script that deploys the agent through a GPO. */
export function createAdKit(payload: AdKitPayload): Promise<CreatedAdKit> {
	return request<CreatedAdKit>('/agent/ad-kit', { method: 'POST', body: payload });
}

/** Stops new enrolments with this token; machines already enrolled keep reporting. */
export function closeAgentToken(id: number): Promise<void> {
	return request<void>(`/agent/tokens/${id}/close`, { method: 'POST' });
}

export function revokeAgentToken(id: number): Promise<void> {
	return request<void>(`/agent/tokens/${id}`, { method: 'DELETE' });
}

/**
 * The agent of a device. `null` when no agent has reported yet, or on an older
 * server without the route: the caller then treats commands as unsupported.
 */
export async function getAgentHost(id: TargetId, signal?: AbortSignal): Promise<AgentHost | null> {
	try {
		return await request<AgentHost>(`/targets/${id}/agent`, { signal, anticipated: true });
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return null;
		throw cause;
	}
}

/**
 * Lets this machine bind itself again at its next batch, for a limited window.
 * The way back in after a reinstall took the agent's own secret with it.
 */
export function allowAgentRebind(id: TargetId): Promise<RebindWindow> {
	return request<RebindWindow>(`/targets/${id}/agent/rebind`, { method: 'POST' });
}

/**
 * Asks the agent to remove itself from the machine (`agent.uninstall`): service,
 * binary and configuration. Administrator session only; `409` when the agent
 * does not run commands, has not reported lately, or a removal is already
 * pending. The monitor stays: follow the command with `getAgentCommand`, and
 * delete the monitor once the agent answered `done`.
 */
export function uninstallAgent(id: TargetId): Promise<AgentCommandView> {
	return request<AgentCommandView>(`/targets/${id}/agent/uninstall`, { method: 'POST' });
}

/** One of the machine's recent commands; `null` once it left the recent list. */
export async function getAgentCommand(
	id: TargetId,
	commandId: number,
	signal?: AbortSignal
): Promise<AgentCommandView | null> {
	const recent = await request<AgentCommandView[]>(`/targets/${id}/commands`, { signal });
	return recent.find((command) => command.id === commandId) ?? null;
}

/** Pulls a command back before the agent picks it up; `409` once it runs or is closed. */
export function cancelAgentCommand(id: TargetId, commandId: number): Promise<void> {
	return request<void>(`/targets/${id}/commands/${commandId}`, { method: 'DELETE' });
}

/** The services the agent can detect on this machine, found or not. Empty on an older server. */
export async function listAgentModules(id: TargetId, signal?: AbortSignal): Promise<AgentModule[]> {
	try {
		return await request<AgentModule[]>(`/targets/${id}/agent/modules`, { signal, anticipated: true });
	} catch (cause) {
		if (cause instanceof ApiError && cause.missing) return [];
		throw cause;
	}
}

/** Gateway of a relay for the agents of its site. */
export interface RelayGatewayChoice {
	enabled: boolean;
	/** Listening port on the relay. Left out: 8443. */
	port?: number | null;
	/** Address the site reaches the relay at. */
	host?: string | null;
}

/** Switches the agent's relay mode on or off; the agent applies it at its next report. */
export function setAgentRelay(
	id: TargetId,
	enabled: boolean,
	site: string | null,
	gateway?: RelayGatewayChoice
): Promise<AgentHost> {
	return request<AgentHost>(`/targets/${id}/agent/relay`, {
		method: 'PUT',
		body: gateway ? { enabled, site, gateway } : { enabled, site }
	});
}

/** Ticks (creates the service's device) or unticks (drops it) a module. */
export function setAgentModule(id: TargetId, module: string, enabled: boolean): Promise<AgentModule> {
	return request<AgentModule>(`/targets/${id}/agent/modules/${encodeURIComponent(module)}`, {
		method: 'PUT',
		body: { enabled }
	});
}
