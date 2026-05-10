import { describe, it, expect } from 'vitest';
import { filterDocuments } from './filter';
import type { AggregatedFile } from '$lib/api/merge';

function makeDoc(overrides: Partial<AggregatedFile> = {}): AggregatedFile {
	return {
		guid: 'g1',
		path: '/books/novel.epub',
		type_: 'EPUB',
		size: 1024,
		fingerprint: 'fp-1',
		tags: [],
		status: 'Unread',
		document_guid: null,
		sourceGuids: { 1: 'g1' },
		...overrides,
	};
}

const library: AggregatedFile[] = [
	makeDoc({ fingerprint: 'fp-1', path: '/docs/quantum-physics.pdf', tags: ['science', 'physics'] }),
	makeDoc({ fingerprint: 'fp-2', path: '/books/great-gatsby.epub', tags: ['fiction', 'classic'] }),
	makeDoc({ fingerprint: 'fp-3', path: '/docs/quantum-computing.pdf', tags: ['science', 'technology'] }),
	makeDoc({ fingerprint: 'fp-4', path: '/books/dune.epub', tags: ['fiction', 'science-fiction'] }),
];

describe('filterDocuments — tag filtering', () => {
	it('returns all documents when no filters are active', () => {
		expect(filterDocuments(library, new Set(), new Set(), '')).toHaveLength(4);
	});

	it('filters to documents that have the allowed tag', () => {
		const result = filterDocuments(library, new Set(['fiction']), new Set(), '');
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['fp-2', 'fp-4']);
	});

	it('requires ALL allowed tags to be present (AND logic)', () => {
		const result = filterDocuments(library, new Set(['science', 'physics']), new Set(), '');
		expect(result.map((d) => d.fingerprint)).toEqual(['fp-1']);
	});

	it('excludes documents that have ANY denied tag (NOT logic)', () => {
		const result = filterDocuments(library, new Set(), new Set(['fiction']), '');
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['fp-1', 'fp-3']);
	});

	it('applies allowed and denied together', () => {
		// allowed: science; denied: physics → only quantum-computing survives
		const result = filterDocuments(library, new Set(['science']), new Set(['physics']), '');
		expect(result.map((d) => d.fingerprint)).toEqual(['fp-3']);
	});

	it('returns empty when allowed tag matches nothing', () => {
		expect(filterDocuments(library, new Set(['no-such-tag']), new Set(), '')).toHaveLength(0);
	});

	it('returns empty when both allowed and denied filters exclude everything', () => {
		// allowed: fiction; denied: fiction → nothing
		expect(filterDocuments(library, new Set(['fiction']), new Set(['fiction']), '')).toHaveLength(0);
	});
});

describe('filterDocuments — search', () => {
	it('returns all documents for an empty search string', () => {
		expect(filterDocuments(library, new Set(), new Set(), '')).toHaveLength(4);
	});

	it('returns all documents for a whitespace-only search string', () => {
		expect(filterDocuments(library, new Set(), new Set(), '   ')).toHaveLength(4);
	});

	it('returns documents that fuzzy-match the query', () => {
		const result = filterDocuments(library, new Set(), new Set(), 'quantum');
		expect(result.length).toBeGreaterThanOrEqual(1);
		result.forEach((d) => expect(d.path).toMatch(/quantum/i));
	});

	it('returns nothing for a completely unrelated query', () => {
		const result = filterDocuments(library, new Set(), new Set(), 'zzzzzzzzzzzzzzzz');
		expect(result).toHaveLength(0);
	});
});

describe('filterDocuments — combined tag + search', () => {
	it('search runs before tag filtering and results compose correctly', () => {
		// "quantum" matches fp-1 and fp-3; allowed: technology narrows to fp-3
		const result = filterDocuments(library, new Set(['technology']), new Set(), 'quantum');
		expect(result.map((d) => d.fingerprint)).toEqual(['fp-3']);
	});

	it('returns empty when search hits docs that are all tag-excluded', () => {
		// "quantum" hits science docs; denied: science → none survive
		const result = filterDocuments(library, new Set(), new Set(['science']), 'quantum');
		expect(result).toHaveLength(0);
	});
});
