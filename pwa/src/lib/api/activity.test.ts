import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ReadFlowClient, __clearTokenCache } from './client';
import type { ActivityPage, ActivityDetail } from './activity';
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

const TOKEN = 'jwt-token-abc';

function tokenResponse(): Response {
	return mockOk({ access_token: TOKEN, token_type: 'Bearer', expires_in: 3600 });
}

/** A fetch mock that answers `/oauth/token` with a token and every other path
 * with `body`. */
function routeFetch(body: unknown) {
	return vi.fn((url: string, _options: RequestInit) => {
		if (url.includes('/oauth/token')) return Promise.resolve(tokenResponse());
		return Promise.resolve(mockOk(body));
	});
}

type FetchCall = [string, RequestInit];

function apiCall(spy: { mock: { calls: FetchCall[] } }): FetchCall | undefined {
	return spy.mock.calls.find(([url]) => !url.includes('/oauth/token'));
}

function op(id: string, started_at: string): ActivityPage {
	return {
		operations: [
			{
				id,
				operation_type: 'scan',
				actor: { kind: 'user', id: 'alice' },
				channel: 'rest',
				status: 'completed',
				dry_run: false,
				started_at,
				completed_at: started_at,
				error_code: null,
				counts: {},
			},
		],
		next_cursor: null,
	};
}

beforeEach(() => {
	__clearTokenCache();
	vi.unstubAllGlobals();
	globalThis.fetch = vi.fn();
});

describe('ReadFlowClient activity endpoints', () => {
	it('fetches the activity page with a limit', async () => {
		const fetchMock = routeFetch(op('op-1', '2026-09-08T10:00:00.000001Z'));
		vi.stubGlobal('fetch', fetchMock);

		const client = new ReadFlowClient(makeSource());
		const page = await client.getActivity(25);

		const [url] = apiCall(fetchMock)!;
		expect(url).toContain('/activity');
		expect(url).toContain('limit=25');
		expect(page.operations).toHaveLength(1);
		expect(page.operations[0].operation_type).toBe('scan');
	});

	it('passes cursor parameters for pagination', async () => {
		const fetchMock = routeFetch(op('op-2', '2026-09-08T09:00:00.000001Z'));
		vi.stubGlobal('fetch', fetchMock);

		const client = new ReadFlowClient(makeSource());
		await client.getActivity(50, { started_at: '2026-09-08T09:00:00.000001Z', operation_id: 'op-1' });

		const [url] = apiCall(fetchMock)!;
		expect(url).toContain('cursor_started_at=');
		expect(url).toContain('cursor_operation_id=');
	});

	it('fetches a single operation detail', async () => {
		const base = op('op-1', '2026-09-08T10:00:00.000001Z').operations[0];
		const detail: ActivityDetail = { ...base, events: [] };
		const fetchMock = routeFetch(detail);
		vi.stubGlobal('fetch', fetchMock);

		const client = new ReadFlowClient(makeSource());
		const result = await client.getActivityDetail('op-1');

		const [url] = apiCall(fetchMock)!;
		expect(url).toContain('/activity/op-1');
		expect(result.operation_type).toBe('scan');
		expect(result.events).toEqual([]);
	});

	it('fetches document-scoped activity', async () => {
		const fetchMock = routeFetch([]);
		vi.stubGlobal('fetch', fetchMock);

		const client = new ReadFlowClient(makeSource());
		const result = await client.getDocumentActivity('guid-abc');

		const [url] = apiCall(fetchMock)!;
		expect(url).toContain('/documents/guid-abc/activity');
		expect(result).toEqual([]);
	});

	it('encodes the operation id for URL safety', async () => {
		const base = op('a/b', 'x').operations[0];
		const detail: ActivityDetail = { ...base, events: [] };
		const fetchMock = routeFetch(detail);
		vi.stubGlobal('fetch', fetchMock);

		const client = new ReadFlowClient(makeSource());
		await client.getActivityDetail('a/b');

		const [url] = apiCall(fetchMock)!;
		expect(url).toContain('/activity/a%2Fb');
	});
});
