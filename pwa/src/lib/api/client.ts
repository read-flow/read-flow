import type { Source } from '$lib/db';

/** String-form reading status as returned by GET /files. */
export type ReadingStatus = 'Unread' | 'Reading' | 'Read';

export type DocumentType =
	| 'Book'
	| 'Article'
	| 'ResearchPaper'
	| 'Thesis'
	| 'Letter'
	| 'Magazine'
	| 'Manual'
	| 'Report';

export interface DocumentMeta {
	document_type: DocumentType | null;
	title: string | null;
	subtitle: string | null;
	authors: string[] | null;
	description: string | null;
	language: string | null;
	publisher: string | null;
	identifier: string | null;
	date: string | null;
	subject: string | null;
	/** Fingerprint of the content whose cover represents the document. */
	selected_cover_fingerprint: string | null;
}

export interface RemoteDocument {
	guid: string;
	metadata: DocumentMeta;
	file_guids: string[];
}

export interface RemoteFile {
	/** Per-server UUID for this file. Use fingerprint for cross-server identity. */
	guid: string;
	path: string;
	type_: string;
	size: number;
	/** Content hash — stable across servers and renames. Used as routing key. */
	fingerprint: string;
	tags: string[];
	/** Populated from the unified reading_state table via JOIN on GET /files. */
	status: ReadingStatus;
	/** UUID of the Document this file belongs to, null if ungrouped. */
	document_guid: string | null;
	/** True when a cover image is stored server-side for this file. */
	has_cover?: boolean;
}

export interface RemoteReadingState {
	fingerprint: string;
	/** Integer encoding: 0 = Unread, 1 = Reading, 2 = Read. */
	status: number;
	/** JSON blob — format is reader-specific (e.g. {"cfi":"..."} for EPUB). */
	position: string;
	percentage: number;
	last_updated: string;
	status_updated_at: string;
}

export interface ServerStatus {
	identifier: string;
	attributes: Record<string, string>;
	nested_checks: ServerStatus[];
}

// ── Admin (server management) ───────────────────────────────────────────────

export interface ScanSummary {
	discovered: number;
	processed: number;
	errors: number;
}

export interface CheckMissingResponse {
	missing: string[];
	purged: boolean;
}

export interface ScanDirectoryEntry {
	path: string;
	action: 'Scan' | 'Ignore';
	/** Present (possibly empty) for Scan entries. */
	tags?: string[];
	inherit: boolean;
}

export interface ServerSettingsDto {
	/** Read-only: returned for display, ignored on PUT. */
	database_url: string;
	extensions: string[];
	dry_run: boolean;
	concurrency: number;
	/** Single-pass extraction of tar archive members during scans. */
	tar_single_pass: boolean;
	private_mode: boolean;
	private_tags: string[];
	/** Origins allowed by CORS (empty = any). */
	allowed_origins: string[];
	/** Maximum upload size in bytes (null = server default). */
	max_upload_bytes: number | null;
}

/** A user as exposed by the API — never includes the password hash. */
export interface UserDto {
	user_id: string;
	roles: string[];
}

// ── Online library (OPDS) ───────────────────────────────────────────────────

export interface DownloadFormat {
	mime_type: string;
	href: string;
	label: string;
}

export interface OnlineBook {
	id: string;
	title: string;
	authors: string[];
	summary: string | null;
	cover_url: string | null;
	formats: DownloadFormat[];
	catalog_name: string;
}

export interface OnlineCatalog {
	name: string;
	/** OPDS search URL; may contain a `{searchTerms}` template placeholder. */
	search_url: string;
	enabled: boolean;
}

export interface OnlineLibrarySearchResponse {
	books: OnlineBook[];
	catalogs: OnlineCatalog[];
}

/** A cached Bearer token, valid until `expiresAt` (epoch ms). */
interface CachedToken {
	token: string;
	expiresAt: number;
}

/**
 * Bearer tokens keyed by `baseUrl|userId`, shared across the short-lived
 * `ReadFlowClient` instances the aggregator creates per call. Kept in memory
 * only; re-obtained after a page reload.
 */
const tokenCache = new Map<string, CachedToken>();

/** Test hook: clear all cached tokens. */
export function __clearTokenCache(): void {
	tokenCache.clear();
}

/** Warn when credentials would be sent over plaintext HTTP to a non-local host.
 * (A browser will also block an HTTP API when the PWA itself is served over
 * HTTPS — mixed content.) */
function warnIfCleartext(baseUrl: string): void {
	try {
		const url = new URL(baseUrl);
		const local = url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '::1';
		if (url.protocol !== 'https:' && !local) {
			console.warn(
				`Read Flow: credentials will be sent over plaintext HTTP to ${baseUrl} — use HTTPS to avoid interception.`,
			);
		}
	} catch {
		// ignore malformed URLs
	}
}

export class ReadFlowClient {
	private baseUrl: string;
	private basicHeader: string;
	private cacheKey: string;
	private privateMode: boolean;

	constructor(source: Source) {
		this.baseUrl = source.baseUrl.replace(/\/$/, '');
		const credentials = btoa(`${source.userId}:${source.passphrase}`);
		this.basicHeader = `Basic ${credentials}`;
		this.cacheKey = `${this.baseUrl}|${source.userId}`;
		this.privateMode = source.privateMode ?? false;
		warnIfCleartext(this.baseUrl);
	}

	/** Obtain a valid Bearer token, exchanging Basic via `/oauth/token`. Returns
	 * `null` if the server has no token endpoint (older server) or on failure,
	 * so callers fall back to Basic. */
	private async ensureToken(): Promise<string | null> {
		const cached = tokenCache.get(this.cacheKey);
		if (cached && cached.expiresAt > Date.now()) return cached.token;
		try {
			const response = await fetch(`${this.baseUrl}/oauth/token`, {
				method: 'POST',
				headers: {
					Authorization: this.basicHeader,
					'Content-Type': 'application/x-www-form-urlencoded',
				},
				body: 'grant_type=password',
			});
			if (!response.ok) return null;
			const data = (await response.json()) as { access_token: string; expires_in: number };
			// Refresh a little early to avoid racing the expiry.
			const expiresAt = Date.now() + (data.expires_in - 30) * 1000;
			tokenCache.set(this.cacheKey, { token: data.access_token, expiresAt });
			return data.access_token;
		} catch {
			return null;
		}
	}

	private async authorization(): Promise<string> {
		const token = await this.ensureToken();
		return token ? `Bearer ${token}` : this.basicHeader;
	}

	/** Fetch with the current auth header (Bearer if available, else Basic) and
	 * the private-mode header. On 401, drop the cached token and retry once. */
	private async authedFetch(path: string, options: RequestInit = {}): Promise<Response> {
		const send = async (): Promise<Response> => {
			const headers: Record<string, string> = { Authorization: await this.authorization() };
			// @feature: remotes.private_mode
			if (this.privateMode) headers['X-Private-Mode'] = 'true';
			return fetch(`${this.baseUrl}${path}`, {
				...options,
				headers: { ...headers, ...(options.headers as Record<string, string> | undefined) },
			});
		};
		let response = await send();
		if (response.status === 401) {
			tokenCache.delete(this.cacheKey);
			response = await send();
		}
		return response;
	}

	private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
		const response = await this.authedFetch(path, {
			...options,
			headers: { 'Content-Type': 'application/json', ...(options.headers as object) },
		});
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}${path}`);
		}
		return response.json() as Promise<T>;
	}

	// For endpoints that return 200 with an empty body (e.g. PUT /reading-progress).
	private async requestVoid(path: string, options: RequestInit = {}): Promise<void> {
		const response = await this.authedFetch(path, {
			...options,
			headers: { 'Content-Type': 'application/json', ...(options.headers as object) },
		});
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}${path}`);
		}
	}

	// @feature: remotes.status
	async status(): Promise<ServerStatus> {
		return this.request<ServerStatus>('/status');
	}

	// @feature: documents.list
	async getFiles(): Promise<RemoteFile[]> {
		return this.request<RemoteFile[]>('/files');
	}

	// @feature: tags.list
	async getAllTags(): Promise<string[]> {
		return this.request<string[]>('/files/tags');
	}

	// @feature: tags.add
	async addTags(guid: string, tags: string[]): Promise<string[]> {
		return this.request<string[]>(`/files/${guid}/tags`, {
			method: 'POST',
			body: JSON.stringify(tags),
		});
	}

	// @feature: tags.remove
	async deleteTags(guid: string, tags: string[]): Promise<string[]> {
		return this.request<string[]>(`/files/${guid}/tags`, {
			method: 'DELETE',
			body: JSON.stringify(tags),
		});
	}

	async getReadingState(fingerprint: string): Promise<RemoteReadingState | null> {
		try {
			return await this.request<RemoteReadingState>(`/reading-state/${fingerprint}`);
		} catch (err) {
			if (err instanceof Error && err.message.includes('HTTP 404')) return null;
			throw err;
		}
	}

	// @feature: reading.progress
	async upsertReadingState(state: RemoteReadingState): Promise<RemoteReadingState> {
		return this.request<RemoteReadingState>('/reading-state', {
			method: 'PUT',
			body: JSON.stringify(state),
		});
	}

	// @feature: reading.status
	async updateReadingStatus(fingerprint: string, status: number): Promise<void> {
		await this.requestVoid(`/reading-state/${fingerprint}/status`, {
			method: 'PUT',
			body: JSON.stringify({ status }),
		});
	}

	async downloadDocumentCover(documentGuid: string): Promise<Blob | null> {
		const response = await this.authedFetch(`/documents/${documentGuid}/cover`);
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`HTTP ${response.status} ${response.statusText}`);
		return response.blob();
	}

	async downloadCover(guid: string): Promise<Blob | null> {
		const response = await this.authedFetch(`/files/${guid}/cover`);
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`HTTP ${response.status} ${response.statusText}`);
		return response.blob();
	}

	// @feature: sources.delete
	async deleteFile(guid: string): Promise<void> {
		await this.requestVoid(`/files/${guid}`, { method: 'DELETE' });
	}

	// @feature: sources.send_to_client
	async uploadFile(blob: Blob, fileName: string): Promise<RemoteFile> {
		const form = new FormData();
		form.append('file', blob, fileName);
		// Multipart: let the browser set Content-Type (with boundary); authedFetch
		// only adds the Authorization + private-mode headers.
		const response = await this.authedFetch('/files', { method: 'POST', body: form });
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}/files`);
		}
		return response.json() as Promise<RemoteFile>;
	}

	async downloadFile(guid: string, fileName: string): Promise<Blob> {
		const response = await this.authedFetch(
			`/files/${guid}/download-as/${encodeURIComponent(fileName)}`,
		);
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText}`);
		}
		return response.blob();
	}

	async ensureDocumentForFile(fileGuid: string): Promise<RemoteDocument> {
		return this.request<RemoteDocument>(`/files/${fileGuid}/document`, { method: 'POST' });
	}

	async getDocuments(): Promise<RemoteDocument[]> {
		return this.request<RemoteDocument[]>('/documents');
	}

	async getDocument(guid: string): Promise<RemoteDocument | null> {
		try {
			return await this.request<RemoteDocument>(`/documents/${guid}`);
		} catch (err) {
			if (err instanceof Error && err.message.includes('HTTP 404')) return null;
			throw err;
		}
	}

	async updateDocumentMetadata(guid: string, meta: DocumentMeta): Promise<RemoteDocument> {
		return this.request<RemoteDocument>(`/documents/${guid}/metadata`, {
			method: 'PUT',
			body: JSON.stringify(meta),
		});
	}

	async mergeDocuments(winnerGuid: string, loserGuids: string[]): Promise<RemoteDocument> {
		return this.request<RemoteDocument>('/documents/merge', {
			method: 'POST',
			body: JSON.stringify({ winner_guid: winnerGuid, loser_guids: loserGuids }),
		});
	}

	// @feature: admin.scan
	async scan(): Promise<ScanSummary> {
		return this.request<ScanSummary>('/scan', { method: 'POST' });
	}

	// @feature: admin.check_missing
	async checkMissing(purge = false): Promise<CheckMissingResponse> {
		return this.request<CheckMissingResponse>(`/maintenance/check-missing?purge=${purge}`, {
			method: 'POST',
		});
	}

	// @feature: admin.scan_directories
	async getScanDirectories(): Promise<ScanDirectoryEntry[]> {
		return this.request<ScanDirectoryEntry[]>('/scan-directories');
	}

	// @feature: admin.scan_directories
	async putScanDirectory(entry: ScanDirectoryEntry): Promise<ScanDirectoryEntry[]> {
		return this.request<ScanDirectoryEntry[]>('/scan-directories', {
			method: 'PUT',
			body: JSON.stringify(entry),
		});
	}

	// @feature: admin.scan_directories
	async deleteScanDirectory(path: string): Promise<ScanDirectoryEntry[]> {
		return this.request<ScanDirectoryEntry[]>(
			`/scan-directories?path=${encodeURIComponent(path)}`,
			{ method: 'DELETE' },
		);
	}

	// @feature: admin.server_settings
	async getSettings(): Promise<ServerSettingsDto> {
		return this.request<ServerSettingsDto>('/settings');
	}

	// @feature: admin.server_settings
	async putSettings(dto: ServerSettingsDto): Promise<ServerSettingsDto> {
		return this.request<ServerSettingsDto>('/settings', {
			method: 'PUT',
			body: JSON.stringify(dto),
		});
	}

	// @feature: admin.authorized_users
	async getUsers(): Promise<UserDto[]> {
		return this.request<UserDto[]>('/users');
	}

	// @feature: admin.authorized_users
	async createUser(userId: string, password: string, roles: string[]): Promise<UserDto[]> {
		return this.request<UserDto[]>('/users', {
			method: 'POST',
			body: JSON.stringify({ user_id: userId, password, roles }),
		});
	}

	// @feature: admin.authorized_users
	async updateUser(userId: string, roles: string[], password?: string): Promise<UserDto[]> {
		return this.request<UserDto[]>(`/users/${encodeURIComponent(userId)}`, {
			method: 'PUT',
			body: JSON.stringify({ roles, ...(password ? { password } : {}) }),
		});
	}

	// @feature: admin.authorized_users
	async deleteUser(userId: string): Promise<UserDto[]> {
		return this.request<UserDto[]>(`/users/${encodeURIComponent(userId)}`, { method: 'DELETE' });
	}

	// @feature: online_library.search
	async searchOnlineLibrary(query: string): Promise<OnlineLibrarySearchResponse> {
		return this.request<OnlineLibrarySearchResponse>(
			`/online-library/search?q=${encodeURIComponent(query)}`,
		);
	}

	// @feature: online_library.download_import
	async importOnlineBook(title: string, format: DownloadFormat): Promise<RemoteFile> {
		return this.request<RemoteFile>('/online-library/import', {
			method: 'POST',
			body: JSON.stringify({ title, format }),
		});
	}
}
