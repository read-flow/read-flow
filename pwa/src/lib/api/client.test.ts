import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ReadFlowClient, __clearTokenCache } from './client';
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

const TOKEN = 'jwt-token-abc';

function tokenResponse(): Response {
	return mockOk({ access_token: TOKEN, token_type: 'Bearer', expires_in: 3600 });
}

/** A fetch mock that answers `/oauth/token` with a token and every other path
 * with `body`. */
function routeFetch(body: unknown) {
	return vi.fn((url: string, _options: RequestInit) => {
		if (url.endsWith('/oauth/token')) return Promise.resolve(tokenResponse());
		return Promise.resolve(mockOk(body));
	});
}

type FetchCall = [string, RequestInit];

/** The first non-token fetch call (the actual API request). */
function apiCall(spy: { mock: { calls: FetchCall[] } }): FetchCall | undefined {
	return spy.mock.calls.find(([url]) => !url.endsWith('/oauth/token'));
}

beforeEach(() => {
	__clearTokenCache();
	vi.unstubAllGlobals();
});

describe('ReadFlowClient — token exchange', () => {
	it('exchanges Basic for a Bearer token, then uses it on the request', async () => {
		const fetchSpy = routeFetch([]);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource({ userId: 'alice', passphrase: 'secret' }));
		await client.getFiles();

		// First call is the token exchange, authenticated with Basic.
		const [tokenUrl, tokenOpts] = fetchSpy.mock.calls[0];
		expect(tokenUrl).toBe('http://localhost:8000/oauth/token');
		expect((tokenOpts.headers as Record<string, string>).Authorization).toBe(
			'Basic ' + btoa('alice:secret'),
		);
		expect(tokenOpts.body).toBe('grant_type=password');

		// The actual request carries the Bearer token.
		const [, opts] = apiCall(fetchSpy)!;
		expect((opts.headers as Record<string, string>).Authorization).toBe(`Bearer ${TOKEN}`);
	});

	it('caches the token across calls (one exchange for multiple requests)', async () => {
		const fetchSpy = routeFetch([]);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.getFiles();
		await client.getFiles();
		const tokenCalls = fetchSpy.mock.calls.filter(([url]) => url.endsWith('/oauth/token'));
		expect(tokenCalls).toHaveLength(1);
	});

	it('retries once with a fresh token on a 401', async () => {
		let apiHits = 0;
		const fetchSpy = vi.fn((url: string, _options: RequestInit) => {
			if (url.endsWith('/oauth/token')) return Promise.resolve(tokenResponse());
			apiHits += 1;
			return Promise.resolve(apiHits === 1 ? mockError(401, 'Unauthorized') : mockOk([]));
		});
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await expect(client.getFiles()).resolves.toEqual([]);
		// token, api(401), token(refresh), api(200)
		expect(apiHits).toBe(2);
		expect(fetchSpy.mock.calls.filter(([u]) => u.endsWith('/oauth/token'))).toHaveLength(2);
	});

	it('falls back to Basic when the server has no token endpoint', async () => {
		const fetchSpy = vi.fn((url: string, _options: RequestInit) => {
			if (url.endsWith('/oauth/token')) return Promise.resolve(mockError(404, 'Not Found'));
			return Promise.resolve(mockOk([]));
		});
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource({ userId: 'alice', passphrase: 'secret' }));
		await client.getFiles();
		const [, opts] = apiCall(fetchSpy)!;
		expect((opts.headers as Record<string, string>).Authorization).toBe(
			'Basic ' + btoa('alice:secret'),
		);
	});
});

describe('ReadFlowClient — constructor', () => {
	it('strips a trailing slash from baseUrl', async () => {
		const fetchSpy = routeFetch([]);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource({ baseUrl: 'http://localhost:8000/' }));
		await client.getFiles();
		expect(fetchSpy).toHaveBeenCalledWith('http://localhost:8000/files', expect.any(Object));
	});

	it('handles special characters in credentials', async () => {
		const fetchSpy = routeFetch([]);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(
			makeSource({ userId: 'user@domain.com', passphrase: 'p@$$!' }),
		);
		await client.getFiles();
		const encoded = (
			fetchSpy.mock.calls[0][1].headers as Record<string, string>
		).Authorization.replace('Basic ', '');
		expect(atob(encoded)).toBe('user@domain.com:p@$$!');
	});
});

describe('ReadFlowClient — getFiles', () => {
	it('calls GET /files and returns parsed JSON', async () => {
		const files = [
			{
				guid: 'g1',
				path: '/a.pdf',
				type_: 'PDF',
				size: 100,
				fingerprint: 'fp',
				tags: [],
				status: 'Unread',
				document_guid: null,
			},
		];
		vi.stubGlobal('fetch', routeFetch(files));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getFiles()).toEqual(files);
	});

	it('throws on a non-OK response', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn((url: string, _options: RequestInit) =>
				Promise.resolve(
					url.endsWith('/oauth/token')
						? tokenResponse()
						: mockError(500, 'Internal Server Error'),
				),
			),
		);
		const client = new ReadFlowClient(makeSource());
		await expect(client.getFiles()).rejects.toThrow('HTTP 500');
	});
});

describe('ReadFlowClient — getAllTags', () => {
	it('calls GET /files/tags and returns a tag list', async () => {
		vi.stubGlobal('fetch', routeFetch(['fiction', 'science']));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getAllTags()).toEqual(['fiction', 'science']);
	});
});

describe('ReadFlowClient — addTags / deleteTags', () => {
	it('POSTs tags to /files/:guid/tags', async () => {
		const fetchSpy = routeFetch(['fiction']);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.addTags('guid-1', ['fiction']);
		const [url, options] = apiCall(fetchSpy)!;
		expect(url).toBe('http://localhost:8000/files/guid-1/tags');
		expect(options.method).toBe('POST');
		expect(JSON.parse(options.body as string)).toEqual(['fiction']);
	});

	it('DELETEs tags from /files/:guid/tags', async () => {
		const fetchSpy = routeFetch([]);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.deleteTags('guid-1', ['fiction']);
		const [url, options] = apiCall(fetchSpy)!;
		expect(url).toBe('http://localhost:8000/files/guid-1/tags');
		expect(options.method).toBe('DELETE');
	});
});

describe('ReadFlowClient — getReadingState', () => {
	const state = {
		fingerprint: 'fp-1',
		status: 1,
		position: '{"cfi":"epubcfi(/6/4)"}',
		percentage: 0.42,
		last_updated: '2024-01-01T00:00:00Z',
		status_updated_at: '2024-01-01T00:00:00Z',
	};

	it('returns the state object on success', async () => {
		vi.stubGlobal('fetch', routeFetch(state));
		const client = new ReadFlowClient(makeSource());
		expect(await client.getReadingState('fp-1')).toEqual(state);
	});

	it('returns null on a 404 response', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn((url: string, _options: RequestInit) =>
				Promise.resolve(url.endsWith('/oauth/token') ? tokenResponse() : mockError(404, 'Not Found')),
			),
		);
		const client = new ReadFlowClient(makeSource());
		expect(await client.getReadingState('fp-missing')).toBeNull();
	});

	it('re-throws non-404 errors', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn((url: string, _options: RequestInit) =>
				Promise.resolve(
					url.endsWith('/oauth/token') ? tokenResponse() : mockError(503, 'Service Unavailable'),
				),
			),
		);
		const client = new ReadFlowClient(makeSource());
		await expect(client.getReadingState('fp-1')).rejects.toThrow('HTTP 503');
	});
});

describe('ReadFlowClient — upsertReadingState', () => {
	it('PUTs state to /reading-state and returns updated state', async () => {
		const state = {
			fingerprint: 'fp-1',
			status: 1,
			position: '{}',
			percentage: 0.5,
			last_updated: '2024-01-01T00:00:00Z',
			status_updated_at: '2024-01-01T00:00:00Z',
		};
		const fetchSpy = routeFetch(state);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		const result = await client.upsertReadingState(state);
		const [url, options] = apiCall(fetchSpy)!;
		expect(url).toBe('http://localhost:8000/reading-state');
		expect(options.method).toBe('PUT');
		expect(JSON.parse(options.body as string)).toEqual(state);
		expect(result).toEqual(state);
	});
});

describe('ReadFlowClient — updateReadingStatus', () => {
	it('PUTs status to /reading-state/:fp/status', async () => {
		const fetchSpy = routeFetch({});
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.updateReadingStatus('fp-1', 2);
		const [url, options] = apiCall(fetchSpy)!;
		expect(url).toBe('http://localhost:8000/reading-state/fp-1/status');
		expect(options.method).toBe('PUT');
		expect(JSON.parse(options.body as string)).toEqual({ status: 2 });
	});
});

describe('ReadFlowClient — downloadFile', () => {
	it('returns a Blob on success', async () => {
		const blob = new Blob(['pdf-content'], { type: 'application/pdf' });
		vi.stubGlobal(
			'fetch',
			vi.fn((url: string, _options: RequestInit) =>
				Promise.resolve(
					url.endsWith('/oauth/token')
						? tokenResponse()
						: ({ ok: true, status: 200, blob: () => Promise.resolve(blob) } as unknown as Response),
				),
			),
		);
		const client = new ReadFlowClient(makeSource());
		expect(await client.downloadFile('guid-1', 'book.pdf')).toBe(blob);
	});

	it('encodes the filename in the URL', async () => {
		const fetchSpy = vi.fn((url: string, _options: RequestInit) =>
			Promise.resolve(
				url.endsWith('/oauth/token')
					? tokenResponse()
					: ({ ok: true, blob: () => Promise.resolve(new Blob()) } as unknown as Response),
			),
		);
		vi.stubGlobal('fetch', fetchSpy);
		const client = new ReadFlowClient(makeSource());
		await client.downloadFile('guid-1', 'my book (2024).pdf');
		const [url] = apiCall(fetchSpy)!;
		expect(url).toContain(encodeURIComponent('my book (2024).pdf'));
	});

	it('throws on a non-OK download response', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn((url: string, _options: RequestInit) =>
				Promise.resolve(url.endsWith('/oauth/token') ? tokenResponse() : mockError(403, 'Forbidden')),
			),
		);
		const client = new ReadFlowClient(makeSource());
		await expect(client.downloadFile('guid-1', 'book.pdf')).rejects.toThrow('HTTP 403');
	});
});
