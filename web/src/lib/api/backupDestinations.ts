/**
 * Off-site destinations of the scheduled local backups (S3-compatible,
 * WebDAV, SFTP). Mirrors `crates/server/src/api/backup_destinations.rs` and
 * `crates/server/src/backup/offsite/`.
 *
 * Secrets never come back: `has_secret` and `saved_secrets` (field names
 * only) say what is stored. On update, omitting `secrets` keeps them and
 * `secrets: {}` clears them.
 */
import { request } from './client';

export type BackupDestinationKind = 's3' | 'webdav' | 'sftp';

export interface S3DestinationSettings {
	/** Empty: AWS (`https://s3.<region>.amazonaws.com`). */
	endpoint: string;
	/** Empty: `us-east-1`. Cloudflare R2 wants `auto`. */
	region: string;
	bucket: string;
	/** Folder inside the bucket, no leading or trailing slash. */
	prefix: string;
	access_key_id: string;
	path_style: boolean;
}

export interface WebDavDestinationSettings {
	/** Address of an existing folder. */
	url: string;
	username: string;
}

export interface SftpDestinationSettings {
	host: string;
	port: number;
	username: string;
	/** Empty: the login folder of the account. */
	directory: string;
	/** Confirmed host key fingerprint (`SHA256:…`); null until the administrator confirms one. */
	host_key: string | null;
}

/** Last upload to a destination. Times are UTC without suffix. */
export interface BackupDestinationResult {
	at: string;
	ok: boolean;
	file: string;
	bytes: number;
	duration_ms: number;
	/** The failure; on a success, a remark (old copies not pruned). */
	error: string | null;
}

export interface BackupDestination {
	id: number;
	name: string;
	/** `s3`, `webdav`, `sftp` — or one this build does not know. */
	kind: BackupDestinationKind | string;
	enabled: boolean;
	keep: number;
	/** `secret.key` is uploaded with the database. */
	include_key: boolean;
	settings: Record<string, unknown>;
	has_secret: boolean;
	saved_secrets: string[];
	last: BackupDestinationResult | null;
	last_success_at: string | null;
}

export interface BackupDestinationList {
	destinations: BackupDestination[];
	/** True while an upload runs. */
	uploading: boolean;
}

export interface BackupDestinationPayload {
	name: string;
	kind: BackupDestinationKind;
	enabled: boolean;
	keep: number;
	include_key: boolean;
	settings: S3DestinationSettings | WebDavDestinationSettings | SftpDestinationSettings;
	/** Omitted: keep the stored secrets. `{}`: clear them. */
	secrets?: Record<string, string>;
}

export interface BackupDestinationTest {
	ok: boolean;
	message: string;
	duration_ms: number;
	/** SFTP fingerprint the server presented when it is not the confirmed one. */
	host_key: string | null;
	/** Another fingerprint was confirmed: reinstalled server, or impostor. */
	host_key_changed: boolean;
}

export function listBackupDestinations(signal?: AbortSignal): Promise<BackupDestinationList> {
	return request<BackupDestinationList>('/backup/destinations', { signal });
}

export function createBackupDestination(payload: BackupDestinationPayload): Promise<BackupDestination> {
	return request<BackupDestination>('/backup/destinations', { method: 'POST', body: payload });
}

export function updateBackupDestination(id: number, payload: BackupDestinationPayload): Promise<BackupDestination> {
	return request<BackupDestination>(`/backup/destinations/${id}`, { method: 'PUT', body: payload });
}

export function deleteBackupDestination(id: number): Promise<void> {
	return request<void>(`/backup/destinations/${id}`, { method: 'DELETE' });
}

export function testBackupDestination(id: number): Promise<BackupDestinationTest> {
	return request<BackupDestinationTest>(`/backup/destinations/${id}/test`, { method: 'POST' });
}

/** Where a destination writes, in a few words: shown in the list. */
export function destinationWhere(destination: BackupDestination): string {
	const s = destination.settings;
	const text = (key: string) => (typeof s[key] === 'string' ? (s[key] as string) : '');
	switch (destination.kind) {
		case 's3': {
			const prefix = text('prefix');
			return `${text('bucket')}${prefix ? `/${prefix}` : ''}`;
		}
		case 'webdav':
			return text('url');
		case 'sftp': {
			const port = typeof s.port === 'number' && s.port !== 22 ? `:${s.port}` : '';
			const directory = text('directory');
			return `${text('username')}@${text('host')}${port}${directory ? `:${directory}` : ''}`;
		}
		default:
			return '';
	}
}
