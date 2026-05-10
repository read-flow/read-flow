import { describe, it, expect } from 'vitest';
import { mergeFiles } from './merge';
import type { RemoteFile } from './client';

function makeFile(overrides: Partial<RemoteFile> = {}): RemoteFile {
	return {
		guid: 'guid-a',
		path: '/books/novel.epub',
		type_: 'EPUB',
		size: 2048,
		fingerprint: 'fp-1',
		tags: [],
		status: 'Unread',
		document_guid: null,
		...overrides,
	};
}

describe('mergeFiles', () => {
	it('returns empty array for no batches', () => {
		expect(mergeFiles([])).toEqual([]);
	});

	it('returns empty array for batches with no files', () => {
		expect(mergeFiles([{ sourceId: 1, files: [] }])).toEqual([]);
	});

	it('returns a single file from one source', () => {
		const file = makeFile();
		const result = mergeFiles([{ sourceId: 1, files: [file] }]);
		expect(result).toHaveLength(1);
		expect(result[0].fingerprint).toBe('fp-1');
		expect(result[0].sourceGuids).toEqual({ 1: 'guid-a' });
	});

	it('keeps distinct files from one source separate', () => {
		const files = [
			makeFile({ guid: 'a', fingerprint: 'fp-1' }),
			makeFile({ guid: 'b', fingerprint: 'fp-2' }),
		];
		const result = mergeFiles([{ sourceId: 1, files }]);
		expect(result).toHaveLength(2);
	});

	it('deduplicates the same fingerprint across two sources', () => {
		const file1 = makeFile({ guid: 'guid-1', fingerprint: 'fp-1' });
		const file2 = makeFile({ guid: 'guid-2', fingerprint: 'fp-1' });
		const result = mergeFiles([
			{ sourceId: 1, files: [file1] },
			{ sourceId: 2, files: [file2] },
		]);
		expect(result).toHaveLength(1);
		expect(result[0].sourceGuids).toEqual({ 1: 'guid-1', 2: 'guid-2' });
	});

	it('uses the first-seen file as the base when merging', () => {
		const file1 = makeFile({ guid: 'guid-1', path: '/a/novel.epub', fingerprint: 'fp-1' });
		const file2 = makeFile({ guid: 'guid-2', path: '/b/novel.epub', fingerprint: 'fp-1' });
		const result = mergeFiles([
			{ sourceId: 1, files: [file1] },
			{ sourceId: 2, files: [file2] },
		]);
		expect(result[0].path).toBe('/a/novel.epub');
	});

	it('unions tags from both sources without duplicates', () => {
		const file1 = makeFile({ fingerprint: 'fp-1', tags: ['fiction', 'classic'] });
		const file2 = makeFile({ fingerprint: 'fp-1', tags: ['classic', 'novel'] });
		const result = mergeFiles([
			{ sourceId: 1, files: [file1] },
			{ sourceId: 2, files: [file2] },
		]);
		expect(result[0].tags.sort()).toEqual(['classic', 'fiction', 'novel']);
	});

	it('handles multiple files and multiple sources simultaneously', () => {
		const source1 = [
			makeFile({ guid: 'a1', fingerprint: 'fp-1', tags: ['fiction'] }),
			makeFile({ guid: 'b1', fingerprint: 'fp-2', tags: [] }),
		];
		const source2 = [
			makeFile({ guid: 'a2', fingerprint: 'fp-1', tags: ['classic'] }),
			makeFile({ guid: 'c1', fingerprint: 'fp-3', tags: ['science'] }),
		];
		const result = mergeFiles([
			{ sourceId: 1, files: source1 },
			{ sourceId: 2, files: source2 },
		]);
		expect(result).toHaveLength(3);

		const fp1 = result.find((f) => f.fingerprint === 'fp-1')!;
		expect(fp1.sourceGuids).toEqual({ 1: 'a1', 2: 'a2' });
		expect(fp1.tags.sort()).toEqual(['classic', 'fiction']);

		const fp2 = result.find((f) => f.fingerprint === 'fp-2')!;
		expect(fp2.sourceGuids).toEqual({ 1: 'b1' });

		const fp3 = result.find((f) => f.fingerprint === 'fp-3')!;
		expect(fp3.sourceGuids).toEqual({ 2: 'c1' });
	});

	it('preserves all original file fields on the merged entry', () => {
		const file = makeFile({
			guid: 'g1',
			path: '/docs/paper.pdf',
			type_: 'PDF',
			size: 4096,
			fingerprint: 'fp-x',
			tags: ['research'],
			status: 'Reading',
			document_guid: 'doc-1',
		});
		const result = mergeFiles([{ sourceId: 5, files: [file] }]);
		const r = result[0];
		expect(r.guid).toBe('g1');
		expect(r.path).toBe('/docs/paper.pdf');
		expect(r.type_).toBe('PDF');
		expect(r.size).toBe(4096);
		expect(r.status).toBe('Reading');
		expect(r.document_guid).toBe('doc-1');
	});
});
