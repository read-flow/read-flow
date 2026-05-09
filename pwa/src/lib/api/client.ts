import type { Source } from '$lib/db';

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

	constructor(source: Source) {
		this.baseUrl = source.baseUrl.replace(/\/$/, '');
		const credentials = btoa(`${source.userId}:${source.passphrase}`);
		this.authHeader = `Basic ${credentials}`;
	}

	private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
		const response = await fetch(`${this.baseUrl}${path}`, {
			...options,
			headers: {
				Authorization: this.authHeader,
				'Content-Type': 'application/json',
				...options.headers,
			},
		});
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText} — ${this.baseUrl}${path}`);
		}
		return response.json() as Promise<T>;
	}

	async status(): Promise<ServerStatus> {
		return this.request<ServerStatus>('/status');
	}

	async getFiles(): Promise<RemoteFile[]> {
		return this.request<RemoteFile[]>('/files');
	}

	async getFile(guid: string): Promise<RemoteFile> {
		return this.request<RemoteFile>(`/files/${guid}`);
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
		await this.request<void>('/reading-progress', {
			method: 'PUT',
			body: JSON.stringify(progress),
		});
	}

	async downloadFile(guid: string, fileName: string): Promise<Blob> {
		const response = await fetch(
			`${this.baseUrl}/files/${guid}/download-as/${encodeURIComponent(fileName)}`,
			{ headers: { Authorization: this.authHeader } },
		);
		if (!response.ok) {
			throw new Error(`HTTP ${response.status} ${response.statusText}`);
		}
		return response.blob();
	}
}
