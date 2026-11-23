// --- Proxmox Backup Server (crates/server/src/api/pbs.rs) --------------------
//
// Times are Unix seconds, as PBS reports them (not server date strings).

export interface PbsSnapshot {
	time: number;
	size: number | null;
	/** `true` verified, `false` verification failed, `null` never verified. */
	verified: boolean | null;
	protected: boolean;
}

export type PbsDayState = 'ok' | 'verify_failed' | 'failed' | 'running' | 'none';

export interface PbsCalendarRun {
	upid: string;
	start: number;
	end: number | null;
	/** `null` while the task is still running. */
	ok: boolean | null;
	status: string | null;
}

export interface PbsCalendarDay {
	/** `YYYY-MM-DD` in the timezone the calendar was asked for. */
	date: string;
	state: PbsDayState;
	/** Backup tasks of that day, newest first. */
	runs: PbsCalendarRun[];
	snapshot: PbsSnapshot | null;
}

export interface PbsCalendarFailure {
	time: number;
	upid: string;
	error: string;
}

export interface PbsCalendarGroup {
	datastore: string;
	namespace: string;
	backup_type: string;
	backup_id: string;
	/** Guest name from the latest snapshot's notes, when PVE wrote one. */
	name: string | null;
	count: number;
	last_time: number | null;
	last_size: number | null;
	last_verified: boolean | null;
	last_success: number | null;
	last_failure: PbsCalendarFailure | null;
	retention: string | null;
	/** One entry per day, oldest first, today last. */
	days: PbsCalendarDay[];
}

export interface PbsCalendar {
	/** `null` before the first successful probe. */
	probed_at: number | null;
	days: number;
	offset_minutes: number;
	groups: PbsCalendarGroup[];
}

export type PbsTaskKind = 'backup' | 'sync' | 'verify' | 'prune' | 'gc' | 'other';

export interface PbsFailure {
	upid: string;
	worker_type: string;
	kind: PbsTaskKind;
	worker_id: string;
	datastore: string | null;
	object: string | null;
	user: string | null;
	start: number;
	end: number | null;
	/** Error message without the `TASK ERROR:` prefix. */
	error: string;
}

export interface PbsJob {
	/** `sync`, `verify`, `prune` or `gc`. */
	kind: string;
	id: string;
	datastore: string;
	namespace: string | null;
	/** Sync jobs only: `remote:remote-store`, or `local`. */
	remote: string | null;
	enabled: boolean;
	schedule: string | null;
	comment: string | null;
	/** Prune jobs only: `last 3, daily 7, weekly 4`. */
	retention: string | null;
	next_run: number | null;
	last_run_state: string | null;
	last_run_end: number | null;
	last_run_upid: string | null;
	/** `null` when the job never ran. */
	last_run_ok: boolean | null;
	error: string | null;
}

export interface PbsJobs {
	probed_at: number | null;
	jobs: PbsJob[];
}

export interface PbsGc {
	last_run_state: string | null;
	last_run_end: number | null;
	last_run_upid: string | null;
	schedule: string | null;
	next_run: number | null;
	removed_bytes: number | null;
	pending_bytes: number | null;
	duration_seconds: number | null;
	disk_chunks: number | null;
	pending_chunks: number | null;
	removed_chunks: number | null;
	/** Chunks the GC could not read and left in place: corruption, not space. */
	bad_chunks: number | null;
}

export interface PbsTypeCount {
	/** `vm`, `ct`, `host` or `other`. */
	backup_type: string;
	groups: number;
	snapshots: number;
}

export interface PbsDatastore {
	name: string;
	available: boolean;
	error: string | null;
	total_bytes: number | null;
	used_bytes: number | null;
	avail_bytes: number | null;
	/** PBS's own fill-up estimate, only when it lies in the future. */
	estimated_full_at: number | null;
	dedup_factor: number | null;
	gc: PbsGc | null;
	/** `nonremovable`, `mounted`, `notmounted`, `unknown`. */
	mount_status: string | null;
	/** `filesystem` or `s3`. */
	backend: string | null;
	/** Maintenance type in force (`offline`, `read-only`…), if any. */
	maintenance: string | null;
	counts: PbsTypeCount[];
	/** Measured growth from PBS's own history; negative when pruning wins. */
	growth_bytes_per_day: number | null;
	/** Days of history the growth and PBS's forecast are computed over. */
	history_days: number | null;
	active_reads: number | null;
	active_writes: number | null;
}

export interface PbsService {
	service: string;
	description: string | null;
	/** `running`, `dead`, `failed`… */
	state: string | null;
	/** `enabled`, `disabled`, `static`, `masked`… */
	unit_state: string | null;
	running: boolean;
	/** `null` for a unit pulled in by another one, which is neither. */
	enabled: boolean | null;
}

export interface PbsPackage {
	package: string;
	title: string | null;
	installed: string | null;
	available: string | null;
	/** Version actually running, for the two packages that report one. */
	running: string | null;
	upgradable: boolean;
	/** `true` when the upgraded package is installed but not yet running. */
	restart_pending: boolean | null;
}

export interface PbsCertificate {
	filename: string;
	subject: string | null;
	issuer: string | null;
	fingerprint: string | null;
	not_after: number | null;
	san: string[];
}

export interface PbsTrafficRule {
	name: string;
	comment: string | null;
	networks: string[];
	timeframe: string[];
	limit_in_bytes: number | null;
	limit_out_bytes: number | null;
	rate_in_bytes: number | null;
	rate_out_bytes: number | null;
}

export interface PbsTapeJob {
	id: string;
	datastore: string;
	namespace: string | null;
	pool: string | null;
	drive: string | null;
	comment: string | null;
	schedule: string | null;
	next_run: number | null;
	next_media_label: string | null;
	last_run_state: string | null;
	last_run_end: number | null;
	last_run_upid: string | null;
}

export interface PbsTapeDrive {
	name: string;
	path: string | null;
	vendor: string | null;
	model: string | null;
	serial: string | null;
	changer: string | null;
	state: string | null;
}

export interface PbsTapeChanger {
	name: string;
	path: string | null;
	vendor: string | null;
	model: string | null;
	serial: string | null;
	export_slots: string | null;
}

export interface PbsMediaPool {
	name: string;
	allocation: string | null;
	retention: string | null;
	comment: string | null;
	encrypted: boolean;
	media_total: number;
	media_expired: number;
	bytes_used: number | null;
}

export interface PbsTapeMedia {
	label: string;
	pool: string | null;
	/** `full`, `writable`, `unknown`, `damaged`, `retired`. */
	status: string | null;
	location: string | null;
	media_set: string | null;
	expired: boolean;
	bytes_used: number | null;
}

export interface PbsTape {
	jobs: PbsTapeJob[];
	drives: PbsTapeDrive[];
	changers: PbsTapeChanger[];
	pools: PbsMediaPool[];
	media: PbsTapeMedia[];
}

export interface PbsDisk {
	name: string;
	devpath: string | null;
	model: string | null;
	serial: string | null;
	size_bytes: number | null;
	disk_type: string | null;
	used: string | null;
	/** `passed`, `failed` or `unknown`. */
	status: string | null;
	/** Endurance used, in percent (SSD only). */
	wearout_percent: number | null;
}

export interface PbsZpool {
	name: string;
	health: string;
	size_bytes: number | null;
	alloc_bytes: number | null;
	free_bytes: number | null;
	fragmentation_percent: number | null;
}

export interface PbsHealth {
	probed_at: number | null;
	version: string | null;
	datastores: PbsDatastore[];
	disks: PbsDisk[];
	zpools: PbsZpool[];
	services: PbsService[];
	packages: PbsPackage[];
	certificates: PbsCertificate[];
	traffic: PbsTrafficRule[];
	/** `null` when the server has no tape tier. */
	tape: PbsTape | null;
}

export interface PbsTaskLog {
	upid: string;
	total: number;
	lines: string[];
}

export interface PbsSmartAttribute {
	id: number | null;
	name: string | null;
	raw: string | null;
	normalized: number | null;
	threshold: number | null;
	worst: number | null;
	flags: string | null;
}

export interface PbsDiskSmart {
	disk: string;
	health: string | null;
	wearout_percent: number | null;
	kind: string | null;
	attributes: PbsSmartAttribute[];
	text: string | null;
}
