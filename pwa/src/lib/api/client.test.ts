import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ReadFlowClient } from './client';
import type { Source } from '$lib/db';

function makeSource(overrides: Partial<Source> = {}): Source {
	return {
		id: 1,
		name: 'Test Source',
		baseUrl: 'http://localhost:8000',
		userId: 'alice',
		passphrase: 'secret',
		order: 0,
		...overrides,
	};
}

function mockOk(body: unknown): Response {
	return {
		ok: true,
		status: 200,
		statusText: 'OK',
		json: () => Promise.resolve(body),
		blob: () => Promise.resolve(new Blob([JSON.stringify(body)])),
	} as unknown as Response;
}

function mockError(status: number, statusText: string): Response {
	return {
		ok: false,
		status,
		statusText,
		json: () => Promise.resolve({}),
	} as unknown as Response;
}

describe('ReadFlowClient — constructor', () => {
	it('strips a trailing slash from baseUrl', () => {
		const client = new ReadFlowClient(makeSource({ baseUrl: 'http://localhost:8000/' }));
		// Verify via the request URL in a fetch call
		const fetchSpy = vi.fn().mockResolvedValue(mockOk([]));
		vi.stubGlobal('fetch', fetchSpy);
		client.getFiles();
		expect(fetchSpy).toHaveBeenCalledWith(
			'http://localhost:8000/files',
			expect.any(Object),
		);
		vi.unstubAllGlobals();
	});

	it('builds a correct Basic auth header', () => {
		const fetchSpy = vi.fn().mockResolvedValue(mockOk([]));
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource({ userId: 'alice', passphrase: 'secret' }));
		client.getFiles();
		const [, options] = fetchSpy.mock.calls[0];
		expect(options.headers.Authorization).toBe('Basic ' + btoa('alice:secret'));
		vi.unstubAllGlobals();
	});

	it('handles special characters in credentials', () => {
		const fetchSpy = vi.fn().mockResolvedValue(mockOk([]));
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource({ userId: 'user@domain.com', passphrase: 'p@$$!' }));
		client.getFiles();
		const [, options] = fetchSpy.mock.calls[0];
		const encoded = options.headers.Authorization.replace('Basic ', '');
		expect(atob(encoded)).toBe('user@domain.com:p@$$!');
		vi.unstubAllGlobals();
	});
});

describe('ReadFlowClient — getFiles', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('calls GET /files and returns parsed JSON', async () => {
		const files = [{ guid: 'g1', path: '/a.pdf', type_: 'PDF', size: 100, fingerprint: 'fp', tags: [], status: 'Unread', document_guid: null }];
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockOk(files)));
		const client = new ReadFlowClient(makeSource());
		const result = await client.getFiles();
		expect(result).toEqual(files);
	});

	it('throws on a non-OK response', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockError(500, 'Internal Server Error')));
		const client = new ReadFlowClient(makeSource());
		await expect(client.getFiles()).rejects.toThrow('HTTP 500');
	});
});

describe('ReadFlowClient — getAllTags', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('calls GET /files/tags and returns a tag list', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockOk(['fiction', 'science'])));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getAllTags()).toEqual(['fiction', 'science']);
	});
});

describe('ReadFlowClient — addTags / deleteTags', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('POSTs tags to /files/:guid/tags', async () => {
		const fetchSpy = vi.fn().mockResolvedValue(mockOk(['fiction']));
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.addTags('guid-1', ['fiction']);
		const [url, options] = fetchSpy.mock.calls[0];
		expect(url).toBe('http://localhost:8000/files/guid-1/tags');
		expect(options.method).toBe('POST');
		expect(JSON.parse(options.body)).toEqual(['fiction']);
	});

	it('DELETEs tags from /files/:guid/tags', async () => {
		const fetchSpy = vi.fn().mockResolvedValue(mockOk([]));
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.deleteTags('guid-1', ['fiction']);
		const [url, options] = fetchSpy.mock.calls[0];
		expect(url).toBe('http://localhost:8000/files/guid-1/tags');
		expect(options.method).toBe('DELETE');
	});
});

describe('ReadFlowClient — getReadingProgress', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('returns the progress object on success', async () => {
		const progress = { fingerprint: 'fp-1', progress: 'epubcfi(/6/4)', last_updated: '2024-01-01T00:00:00Z' };
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockOk(progress)));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getReadingProgress('fp-1')).toEqual(progress);
	});

	it('returns null on a 404 response', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockError(404, 'Not Found')));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getReadingProgress('fp-missing')).toBeNull();
	});

	it('re-throws non-404 errors', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockError(503, 'Service Unavailable')));
		const client = new ReadFlowClient(makeSource());
		await expect(client.getReadingProgress('fp-1')).rejects.toThrow('HTTP 503');
	});
});

describe('ReadFlowClient — upsertReadingProgress', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('PUTs progress to /reading-progress', async () => {
		const fetchSpy = vi.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		const progress = { fingerprint: 'fp-1', progress: 'epubcfi(/6/4)', last_updated: '2024-01-01T00:00:00Z' };
		await client.upsertReadingProgress(progress);
		const [url, options] = fetchSpy.mock.calls[0];
		expect(url).toBe('http://localhost:8000/reading-progress');
		expect(options.method).toBe('PUT');
		expect(JSON.parse(options.body)).toEqual(progress);
	});
});

describe('ReadFlowClient — downloadFile', () => {
	beforeEach(() => vi.unstubAllGlobals());

	it('returns a Blob on success', async () => {
		const blob = new Blob(['pdf-content'], { type: 'application/pdf' });
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			blob: () => Promise.resolve(blob),
		} as unknown as Response));
		const client = new ReadFlowClient(makeSource());
		const result = await client.downloadFile('guid-1', 'book.pdf');
		expect(result).toBe(blob);
	});

	it('encodes the filename in the URL', async () => {
		const fetchSpy = vi.fn().mockResolvedValue({
			ok: true,
			blob: () => Promise.resolve(new Blob()),
		} as unknown as Response);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.downloadFile('guid-1', 'my book (2024).pdf');
		const [url] = fetchSpy.mock.calls[0];
		expect(url).toContain(encodeURIComponent('my book (2024).pdf'));
	});

	it('throws on a non-OK download response', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockError(403, 'Forbidden')));
		const client = new ReadFlowClient(makeSource());
		await expect(client.downloadFile('guid-1', 'book.pdf')).rejects.toThrow('HTTP 403');
	});
});
