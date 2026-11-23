import type { TargetId } from './targets.js';

// --- Relay agents (remote sites) --------------------------------------------

/**
 * `GET /api/relays`: an agent as the device form offers it under "Reached
 * through". Mirrors `RelayView` in `crates/server/src/api/relay.rs`.
 */
export interface RelayAgent {
	/** Id of the agent's target. */
	id: TargetId;
	name: string;
	site: string | null;
	/**
	 * True when the agent declared `relay: true` at its last batch. An agent
	 * picked as relay without it never fetches the probes: the UI warns.
	 */
	relay: boolean;
	last_seen_at: string | null;
	/** Devices reached through this agent. */
	relayed: number;
	/** Gateway port asked from the UI; `null` when closed. Absent on an older server. */
	gateway_wanted?: number | null;
	/** Port the relay reports its gateway listening on. */
	gateway_port?: number | null;
	/** Address the site reaches the relay at. */
	gateway_host?: string | null;
	/** Client of the agent: a relay only carries machines of its own client. */
	client_id?: number | null;
}

// --- Topology map -------------------------------------------------------------

/** Where a link of the map comes from. */
export type TopologySource = 'parent' | 'service' | 'relay' | 'hypervisor' | 'hosted' | 'lldp' | 'l2' | 'samehost' | 'backup' | 'firewall' | 'manual';

/** Who drew a link: the server's deduction, a correction by hand, or a learned rule. */
export type TopologyOrigin = 'deduced' | 'manual' | 'rule';

/** Where a node's site comes from: the location it is placed at, its `site` tag, an agent's declaration, or the relay agent that polls it. */
export type TopologySiteSource = 'tag' | 'agent' | 'relay' | 'location' | 'manual';

/** A node of `GET /api/topology`. Mirrors `NodeView` in `crates/server/src/api/topology.rs`. */
export interface TopologyNode {
	/** `t<id>` for a device, `g<host>:<name>` for an unmonitored guest. */
	id: string;
	/** The device; `null` for a guest. */
	target_id: TargetId | null;
	name: string;
	/** The device kind; `vm` or `container` for a guest, `pve_node` for a node of a Proxmox cluster. */
	kind: string;
	/** Folder; a guest takes its hypervisor's. */
	group_name: string;
	/** `manual` when the grouping was corrected on the map. */
	group_source?: 'manual' | null;
	/** Remote site the node belongs to; `null` for the main site. */
	site: string | null;
	site_source: TopologySiteSource | null;
	/** Guest only: the hypervisor device. */
	host_id: TargetId | null;
	/** Subnet of the device (CIDR); `null` for a guest, a node, a public address or a host name. */
	subnet: string | null;
	/** VLAN of that subnet, when a firewall reports it. */
	vlan: number | null;
	/** Name of that network (the firewall interface description). */
	vlan_name: string | null;
	/** Guest: running at the last measurement. Cluster node: online. */
	running: boolean | null;
	/** Guest (its own node, or the device that stands for it): the addresses its hypervisor reports. */
	ips: string[];
}

/** A link: `from` is upstream (parent, relay, hypervisor, switch). Mirrors `EdgeView`. */
export interface TopologyEdge {
	from: string;
	to: string;
	source: TopologySource;
	origin?: TopologyOrigin;
	/** The correction that drew it. */
	correction_id?: number | null;
	/** The learned rule that drew it. */
	rule_id?: number | null;
	/** Switch port on the `from` / `to` side (`lldp` and `l2` links), read from the switch tables. */
	from_port?: string | null;
	to_port?: string | null;
	/** Weathermap: traffic of the link's port, when its interface counters are known. */
	traffic?: TopologyEdgeTraffic | null;
}

/** Traffic of a link, read from `if_octets_*` and `if_speed_bps`. Mirrors `EdgeTraffic` in `api/topology_traffic.rs`. */
export interface TopologyEdgeTraffic {
	/** Node (`t<id>`) whose counters were read: `from` or `to`. */
	measured_on: string;
	/** Interface names measured. */
	ports: string[];
	/** Bits per second received / sent by `measured_on`. */
	bps_in: number;
	bps_out: number;
	/** Negotiated speed, bits per second; `null` when a port does not report it. */
	speed_bps: number | null;
	/** Rate over speed, 0 to 1; `null` without a speed. */
	utilization_in: number | null;
	utilization_out: number | null;
	/** Errors and discards per second, both directions. */
	errors_per_sec: number;
}

/** `GET /api/targets/{id}/port-traffic`: the last hour of a port's traffic. */
export interface PortTraffic {
	step_secs: number;
	points: { t: number; bps_in: number; bps_out: number }[];
}

export type TopologyCorrectionKind = 'forbid' | 'link' | 'parent' | 'site' | 'group';

/** A correction made by hand on the map. Mirrors `Correction` in `api/topology_edits.rs`. */
export interface TopologyCorrection {
	id: number;
	kind: TopologyCorrectionKind;
	/** The new parent, or one end of the link. */
	from: string | null;
	to: string;
	/** Type of the link forbidden or drawn. */
	source: TopologySource | null;
	/** Site (empty: main site) or grouping. */
	value: string | null;
	/** `parent`: also the device's declared parent (alert suppression). */
	use_for_alerts: boolean;
	author: string;
	created_at: string;
}

/** `POST /api/topology/corrections`. */
export interface TopologyCorrectionInput {
	kind: TopologyCorrectionKind;
	from?: string | null;
	to: string;
	source?: TopologySource | null;
	value?: string | null;
	use_for_alerts?: boolean;
}

export interface TopologyCorrectionCreated {
	correction: TopologyCorrection;
	/** Corrections it replaced (same node or same pair), for an undo. */
	replaced: TopologyCorrection[];
}

/** Scope and effect of a map rule. Mirrors `RuleSpec`. */
export interface TopologyRuleSpec {
	kind: 'forbid' | 'parent';
	subnet: string | null;
	address: string | null;
	target_kind: string | null;
	/** `forbid`: the type of link never deduced in the scope. */
	source: TopologySource | null;
	/** `parent`: the node the scope depends on. */
	parent: string | null;
}

export type TopologyRuleStatus = 'active' | 'dismissed';

export interface TopologyRule extends TopologyRuleSpec {
	id: number;
	status: TopologyRuleStatus;
	evidence: number;
	author: string;
	created_at: string;
	/** Links it sets or removes on today's map. */
	matches: number;
}

/** A rule the corrections suggest; never applied until accepted. */
export interface TopologySuggestion extends TopologyRuleSpec {
	key: string;
	evidence: number;
	corrections: number[];
	/** Other links it would change on today's map. */
	matches: number;
}

/** `GET /api/topology`. Mirrors `TopologyView`. */
export interface Topology {
	nodes: TopologyNode[];
	edges: TopologyEdge[];
	/** False when VictoriaMetrics did not answer: hypervisor and LLDP links are missing. */
	metrics_available: boolean;
	/** Corrections made by hand, already applied to `nodes` and `edges`. */
	corrections?: TopologyCorrection[];
	rules?: TopologyRule[];
	suggestions?: TopologySuggestion[];
}

// --- World map locations (`api/locations.rs`) ---------------------------------

export type LocationId = number;

/** How a device came to a location: its assignment, its site, or the main site. */
export type LocationSource = 'assigned' | 'site' | 'main';

export interface LocationMember {
	target_id: TargetId;
	source: LocationSource;
}

/** One location. Mirrors `LocationView`. */
export interface MapLocation {
	id: LocationId;
	name: string;
	/** Two-character badge code, upper case; never empty. */
	code: string;
	/** Free text, shown only. */
	address: string;
	latitude: number;
	longitude: number;
	/** Topology site this location stands for; empty: none. */
	site: string;
	members: LocationMember[];
}

/** `GET /api/locations`. Mirrors `LocationsView`. */
export interface Locations {
	locations: MapLocation[];
	/** Remote sites the topology map knows. */
	sites: string[];
	/** Devices no location gathers. */
	unplaced: TargetId[];
}

/** Body of `POST /api/locations` and `PUT /api/locations/{id}`. */
export interface LocationPayload {
	name: string;
	address: string;
	latitude: number;
	longitude: number;
	site: string;
	/** Badge code; empty: the first two letters of the name. */
	code?: string;
}

export type BasemapMode = 'offline' | 'osm' | 'custom';

/** `GET|PUT /api/locations/basemap`. Mirrors `Basemap`. */
export interface Basemap {
	mode: BasemapMode;
	url: string;
	attribution: string;
}

// --- OPNsense live map (`api/opnsense_live.rs`) and IP geolocation ------------

/** `GET|PUT /api/geoip`. Mirrors `GeoStatus`. */
export interface GeoipStatus {
	enabled: boolean;
	state: 'ready' | 'loading' | 'missing' | 'failed' | 'disabled';
	/** Month of the loaded database (`2026-10`). */
	month: string | null;
	ranges: number;
	error: string | null;
	source: string;
	source_url: string;
}

export interface OpnsenseLiveTally {
	blocked: number;
	passed: number;
	other: number;
}

/** One connection, reduced: never an address. Mirrors `LiveEvent`. */
export interface OpnsenseLiveEvent {
	seq: number;
	/** Unix seconds. */
	at: number;
	action: 'block' | 'pass' | 'other';
	/** From the local network out. */
	outbound: boolean;
	/** `CH`, `local` or `unknown`. */
	country: string;
	proto: string;
	port: number | null;
	rule: string;
}

/** `GET /api/targets/{id}/opnsense/live`. Mirrors `LiveView`. */
export interface OpnsenseLive {
	/** False while the monitor's `live_map` option is off. */
	enabled: boolean;
	/** Polled through a relay agent: the server cannot read the log itself. */
	relayed: boolean;
	window_secs: number;
	polled_at: number | null;
	error: string | null;
	totals: OpnsenseLiveTally;
	countries: (OpnsenseLiveTally & { code: string })[];
	rules: { rule: string; action: 'block' | 'pass' | 'other'; count: number }[];
	ports: { proto: string; port: number; count: number }[];
	recent: OpnsenseLiveEvent[];
	seq: number;
	/** Where the firewall sits on the world map, when it has a location. */
	firewall: { name: string; latitude: number; longitude: number } | null;
	geoip: GeoipStatus;
}
