import type { Source } from '$lib/db';

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
}

export interface RemoteDocument {
	guid: string;
	metadata: DocumentMeta;
	file_guids: string[];
}

export interface RemoteFile {
	guid: string;
	path: string;
	type_: string;
	size: number;
	fingerprint: string;
	tags: string[];
	status: 'Unread' | 'Reading' | 'Read';
	document_guid: string | null;
}

export interface RemoteReadingProgress {
	fingerprint: string;
	progress: string;
	last_updated: string;
}

export interface ServerStatus {
	identifier: string;
	attributes: Record<string, string>;
	nested_checks: ServerStatus[];
}

export class ReadFlowClient {
	private baseUrl: string;
	private authHeader: string;
	private privateMode: boolean;

	constructor(source: Source) {
		this.baseUrl = source.baseUrl.replace(/\/$/, '');
		const credentials = btoa(`${source.userId}:${source.passphrase}`);
		this.authHeader = `Basic ${credentials}`;
		this.privateMode = source.privateMode ?? false;
	}

	private headers(extra?: HeadersInit): HeadersInit {
		const base: Record<string, string> = {
			Authorization: this.authHeader,
			'Content-Type': 'application/json',
		};
		if (this.privateMode) {
			base['X-Private-Mode'] = 'true';
		}
		return { ...base, ...extra };
	}

	private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
		const response = await fetch(`${this.baseUrl}${path}`, {
			...options,
			headers: this.headers(options.headers),
		});
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}${path}`);
		}
		return response.json() as Promise<T>;
	}

	// For endpoints that return 200 with an empty body (e.g. PUT /reading-progress).
	private async requestVoid(path: string, options: RequestInit = {}): Promise<void> {
		const response = await fetch(`${this.baseUrl}${path}`, {
			...options,
			headers: this.headers(options.headers),
		});
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}${path}`);
		}
	}

	async status(): Promise<ServerStatus> {
		return this.request<ServerStatus>('/status');
	}

	async getFiles(): Promise<RemoteFile[]> {
		return this.request<RemoteFile[]>('/files');
	}

	async getAllTags(): Promise<string[]> {
		return this.request<string[]>('/files/tags');
	}

	async addTags(guid: string, tags: string[]): Promise<string[]> {
		return this.request<string[]>(`/files/${guid}/tags`, {
			method: 'POST',
			body: JSON.stringify(tags),
		});
	}

	async deleteTags(guid: string, tags: string[]): Promise<string[]> {
		return this.request<string[]>(`/files/${guid}/tags`, {
			method: 'DELETE',
			body: JSON.stringify(tags),
		});
	}

	async getReadingProgress(fingerprint: string): Promise<RemoteReadingProgress | null> {
		try {
			return await this.request<RemoteReadingProgress>(`/reading-progress/${fingerprint}`);
		} catch (err) {
			if (err instanceof Error && err.message.includes('HTTP 404')) return null;
			throw err;
		}
	}

	async upsertReadingProgress(progress: RemoteReadingProgress): Promise<void> {
		await this.requestVoid('/reading-progress', {
			method: 'PUT',
			body: JSON.stringify(progress),
		});
	}

	async downloadFile(guid: string, fileName: string): Promise<Blob> {
		const response = await fetch(
			`${this.baseUrl}/files/${guid}/download-as/${encodeURIComponent(fileName)}`,
			{ headers: this.headers() },
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

}
