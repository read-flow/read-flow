import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Hoisted mocks (evaluated before imports) ──────────────────────────────────

const mocks = vi.hoisted(() => ({
	sourcesToArray: vi.fn(),
	readingStateGet: vi.fn(),
	readingStatePut: vi.fn(),
	readingStateWhere: vi.fn(),
	MockReadFlowClient: vi.fn(),
}));

vi.mock('$lib/db', () => ({
	db: {
		sources: {
			orderBy: () => ({ toArray: mocks.sourcesToArray }),
		},
		readingState: {
			get: mocks.readingStateGet,
			put: mocks.readingStatePut,
			where: () => ({ equals: () => ({ modify: mocks.readingStateWhere }) }),
		},
	},
}));

vi.mock('./client', () => ({
	// Wrap in a constructible function: vitest no longer allows `new` on a
	// vi.fn() whose implementations are arrow functions.
	ReadFlowClient: function (this: unknown, ...args: unknown[]) {
		return mocks.MockReadFlowClient(...args);
	},
}));

import {
	fetchAllFiles,
	fetchAllTags,
	addTagsToFile,
	removeTagsFromFile,
	fetchReadingState,
	saveReadingState,
	downloadFileFromSources,
} from './aggregator';
import type { Source } from '$lib/db';
import type { RemoteFile, RemoteReadingState } from './client';

// ── Fixtures ───────────────────────────────────────────────────────────────────

function makeSource(id: number, overrides: Partial<Source> = {}): Source {
	return {
		id,
		name: `Source ${id}`,
		baseUrl: `http://source${id}.local`,
		userId: 'alice',
		passphrase: 'secret',
		order: id - 1,
		privateMode: false,
		...overrides,
	};
}

function makeRemoteFile(overrides: Partial<RemoteFile> = {}): RemoteFile {
	return {
		guid: 'guid-a',
		path: '/books/novel.epub',
		type_: 'epub',
		size: 2048,
		fingerprint: 'fp-1',
		tags: [],
		status: 'Unread',
		document_guid: null,
		...overrides,
	};
}

// ── fetchAllFiles ──────────────────────────────────────────────────────────────

describe('fetchAllFiles', () => {
	beforeEach(() => vi.clearAllMocks());

	it('returns empty array when no sources are configured', async () => {
		mocks.sourcesToArray.mockResolvedValue([]);
		expect(await fetchAllFiles()).toEqual([]);
	});

	it('returns files from a single source with sourceGuids populated', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({
			getFiles: vi.fn().mockResolvedValue([makeRemoteFile()]),
		}));
		const result = await fetchAllFiles();
		expect(result).toHaveLength(1);
		expect(result[0].sourceGuids).toEqual({ 1: 'guid-a' });
	});

	it('merges the same file from two sources', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockResolvedValue([makeRemoteFile({ guid: 'g1', fingerprint: 'fp-1' })]),
			}))
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockResolvedValue([makeRemoteFile({ guid: 'g2', fingerprint: 'fp-1' })]),
			}));
		const result = await fetchAllFiles();
		expect(result).toHaveLength(1);
		expect(result[0].sourceGuids).toEqual({ 1: 'g1', 2: 'g2' });
	});

	it('returns distinct files from two sources without merging', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockResolvedValue([makeRemoteFile({ fingerprint: 'fp-1' })]),
			}))
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockResolvedValue([makeRemoteFile({ guid: 'g2', fingerprint: 'fp-2' })]),
			}));
		const result = await fetchAllFiles();
		expect(result).toHaveLength(2);
	});

	it('tolerates a source that fails and returns files from the working source', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockResolvedValue([makeRemoteFile({ fingerprint: 'fp-ok' })]),
			}))
			.mockImplementationOnce(() => ({
				getFiles: vi.fn().mockRejectedValue(new Error('network timeout')),
			}));
		const result = await fetchAllFiles();
		expect(result).toHaveLength(1);
		expect(result[0].fingerprint).toBe('fp-ok');
	});
});

// ── fetchAllTags ───────────────────────────────────────────────────────────────

describe('fetchAllTags', () => {
	beforeEach(() => vi.clearAllMocks());

	it('returns a sorted union of tags from all sources', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({ getAllTags: vi.fn().mockResolvedValue(['b', 'a']) }))
			.mockImplementationOnce(() => ({ getAllTags: vi.fn().mockResolvedValue(['c', 'a']) }));
		expect(await fetchAllTags()).toEqual(['a', 'b', 'c']);
	});

	it('returns empty array when no sources are configured', async () => {
		mocks.sourcesToArray.mockResolvedValue([]);
		expect(await fetchAllTags()).toEqual([]);
	});
});

// ── addTagsToFile ──────────────────────────────────────────────────────────────

describe('addTagsToFile', () => {
	beforeEach(() => vi.clearAllMocks());

	it('calls addTags on each source listed in sourceGuids', async () => {
		const addTags1 = vi.fn().mockResolvedValue([]);
		const addTags2 = vi.fn().mockResolvedValue([]);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({ addTags: addTags1 }))
			.mockImplementationOnce(() => ({ addTags: addTags2 }));
		await addTagsToFile({ 1: 'guid-s1', 2: 'guid-s2' }, ['fiction']);
		expect(addTags1).toHaveBeenCalledWith('guid-s1', ['fiction']);
		expect(addTags2).toHaveBeenCalledWith('guid-s2', ['fiction']);
	});

	it('skips sources that are not in sourceGuids', async () => {
		const addTags3 = vi.fn();
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2), makeSource(3)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({ addTags: vi.fn().mockResolvedValue([]) }))
			.mockImplementationOnce(() => ({ addTags: vi.fn().mockResolvedValue([]) }));
		await addTagsToFile({ 1: 'g1', 2: 'g2' }, ['science']);
		// Source 3 never gets a client constructed, so addTags3 is never registered
		expect(addTags3).not.toHaveBeenCalled();
	});
});

// ── removeTagsFromFile ─────────────────────────────────────────────────────────

describe('removeTagsFromFile', () => {
	beforeEach(() => vi.clearAllMocks());

	it('calls deleteTags on each source listed in sourceGuids', async () => {
		const deleteTags = vi.fn().mockResolvedValue([]);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({ deleteTags }));
		await removeTagsFromFile({ 1: 'guid-s1' }, ['old-tag']);
		expect(deleteTags).toHaveBeenCalledWith('guid-s1', ['old-tag']);
	});

	it('skips sources that are not in sourceGuids', async () => {
		const deleteTags2 = vi.fn();
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({ deleteTags: vi.fn().mockResolvedValue([]) }));
		await removeTagsFromFile({ 1: 'g1' }, ['tag']);
		expect(deleteTags2).not.toHaveBeenCalled();
	});
});

// ── fetchReadingState ──────────────────────────────────────────────────────────

function makeRemoteState(overrides: Partial<RemoteReadingState> = {}): RemoteReadingState {
	return {
		fingerprint: 'fp-1',
		status: 0,
		position: '{}',
		percentage: 0,
		last_updated: '2024-01-01T00:00:00Z',
		status_updated_at: '1970-01-01T00:00:00Z',
		...overrides,
	};
}

describe('fetchReadingState', () => {
	beforeEach(() => vi.clearAllMocks());

	it('returns null when there is no local record and no sources', async () => {
		mocks.readingStateGet.mockResolvedValue(undefined);
		mocks.sourcesToArray.mockResolvedValue([]);
		expect(await fetchReadingState('fp-1')).toBeNull();
	});

	it('returns the local record when no remote sources are configured', async () => {
		mocks.readingStateGet.mockResolvedValue({
			fingerprint: 'fp-1', status: 'Reading', position: '{"cfi":"x"}',
			percentage: 0.3, lastUpdated: '2024-01-01T00:00:00Z', statusUpdatedAt: '2024-01-01T00:00:00Z',
		});
		mocks.sourcesToArray.mockResolvedValue([]);
		const result = await fetchReadingState('fp-1');
		expect(result?.percentage).toBe(0.3);
	});

	it('returns the remote record when it is newer than the local one', async () => {
		mocks.readingStateGet.mockResolvedValue({
			fingerprint: 'fp-1', status: 'Unread', position: '{}',
			percentage: 0, lastUpdated: '2024-01-01T00:00:00Z', statusUpdatedAt: '1970-01-01T00:00:00Z',
		});
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		const remote = makeRemoteState({ percentage: 0.9, last_updated: '2024-06-01T00:00:00Z' });
		mocks.MockReadFlowClient.mockImplementation(() => ({
			getReadingState: vi.fn().mockResolvedValue(remote),
		}));
		const result = await fetchReadingState('fp-1');
		expect(result?.percentage).toBe(0.9);
	});

	it('keeps the local record when it is newer than the remote one', async () => {
		mocks.readingStateGet.mockResolvedValue({
			fingerprint: 'fp-1', status: 'Reading', position: '{}',
			percentage: 0.8, lastUpdated: '2024-12-01T00:00:00Z', statusUpdatedAt: '2024-01-01T00:00:00Z',
		});
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({
			getReadingState: vi.fn().mockResolvedValue(makeRemoteState({ percentage: 0.1, last_updated: '2024-01-01T00:00:00Z' })),
		}));
		const result = await fetchReadingState('fp-1');
		expect(result?.percentage).toBe(0.8);
	});

	it('returns remote record when no local record exists', async () => {
		mocks.readingStateGet.mockResolvedValue(undefined);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		const remote = makeRemoteState({ percentage: 0.5, last_updated: '2024-03-01T00:00:00Z' });
		mocks.MockReadFlowClient.mockImplementation(() => ({
			getReadingState: vi.fn().mockResolvedValue(remote),
		}));
		const result = await fetchReadingState('fp-1');
		expect(result?.percentage).toBe(0.5);
	});
});

// ── saveReadingState ───────────────────────────────────────────────────────────

describe('saveReadingState', () => {
	beforeEach(() => vi.clearAllMocks());

	it('writes to local DB immediately', async () => {
		mocks.readingStatePut.mockResolvedValue(undefined);
		mocks.sourcesToArray.mockResolvedValue([]);
		const state = makeRemoteState({ percentage: 0.2 });
		await saveReadingState(state);
		expect(mocks.readingStatePut).toHaveBeenCalledWith(
			expect.objectContaining({ fingerprint: 'fp-1', percentage: 0.2 }),
		);
	});

	it('fans out to all remote sources and returns the server result', async () => {
		mocks.readingStatePut.mockResolvedValue(undefined);
		const serverResult = makeRemoteState({ status: 1, percentage: 0.5 });
		const upsert1 = vi.fn().mockResolvedValue(serverResult);
		const upsert2 = vi.fn().mockResolvedValue(serverResult);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({ upsertReadingState: upsert1 }))
			.mockImplementationOnce(() => ({ upsertReadingState: upsert2 }));
		const state = makeRemoteState();
		const result = await saveReadingState(state);
		expect(upsert1).toHaveBeenCalledWith(state);
		expect(result.status).toBe(1);
	});

	it('does not throw when a remote source fails to save', async () => {
		mocks.readingStatePut.mockResolvedValue(undefined);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({
			upsertReadingState: vi.fn().mockRejectedValue(new Error('server down')),
		}));
		await expect(saveReadingState(makeRemoteState())).resolves.toBeDefined();
	});
});

// ── downloadFileFromSources ────────────────────────────────────────────────────

describe('downloadFileFromSources', () => {
	beforeEach(() => vi.clearAllMocks());

	it('downloads from the first source that has the file', async () => {
		const blob = new Blob(['content']);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1)]);
		mocks.MockReadFlowClient.mockImplementation(() => ({
			downloadFile: vi.fn().mockResolvedValue(blob),
		}));
		const result = await downloadFileFromSources({ 1: 'guid-a' }, 'book.pdf');
		expect(result).toBe(blob);
	});

	it('falls back to the next source when the first fails', async () => {
		const blob = new Blob(['content']);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({
				downloadFile: vi.fn().mockRejectedValue(new Error('HTTP 503')),
			}))
			.mockImplementationOnce(() => ({
				downloadFile: vi.fn().mockResolvedValue(blob),
			}));
		const result = await downloadFileFromSources({ 1: 'g1', 2: 'g2' }, 'book.pdf');
		expect(result).toBe(blob);
	});

	it('throws a combined error when all sources fail', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		mocks.MockReadFlowClient
			.mockImplementationOnce(() => ({
				downloadFile: vi.fn().mockRejectedValue(new Error('HTTP 503')),
			}))
			.mockImplementationOnce(() => ({
				downloadFile: vi.fn().mockRejectedValue(new Error('HTTP 404')),
			}));
		await expect(
			downloadFileFromSources({ 1: 'g1', 2: 'g2' }, 'book.pdf'),
		).rejects.toThrow('Could not download "book.pdf" from any source');
	});

	it('skips sources that are not in sourceGuids', async () => {
		const blob = new Blob(['content']);
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		// Source 1 is NOT in sourceGuids, so only source 2 gets a client
		mocks.MockReadFlowClient.mockImplementation(() => ({
			downloadFile: vi.fn().mockResolvedValue(blob),
		}));
		const result = await downloadFileFromSources({ 2: 'guid-b' }, 'file.epub');
		expect(result).toBe(blob);
		// Verify only one client was instantiated (for source 2)
		expect(mocks.MockReadFlowClient).toHaveBeenCalledTimes(1);
	});

	it('throws when no source holds the file', async () => {
		mocks.sourcesToArray.mockResolvedValue([makeSource(1), makeSource(2)]);
		await expect(
			downloadFileFromSources({}, 'missing.pdf'),
		).rejects.toThrow('Could not download "missing.pdf" from any source');
	});
});
