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
		otherFormats: [],
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
	it('tag filter and text search compose (AND logic)', () => {
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

// Fixtures with deeply-nested paths to exercise the basename search fix.
const deepLibrary: AggregatedFile[] = [
	makeDoc({
		fingerprint: 'dp-1',
		path: '/books/great-gatsby.epub',
		tags: ['fiction'],
	}),
	makeDoc({
		fingerprint: 'dp-2',
		path: '/home/user/documents/fiction/classics/great-gatsby.epub',
		tags: ['fiction'],
	}),
	makeDoc({
		fingerprint: 'dp-3',
		path: '/docs/quantum-physics.pdf',
		tags: ['science'],
	}),
	makeDoc({
		fingerprint: 'dp-4',
		path: '/home/user/very/deep/nested/path/quantum-physics.pdf',
		tags: ['science'],
	}),
];

describe('filterDocuments — search: basename matching', () => {
	it('returns the same results for shallow and deeply-nested files with the same filename', () => {
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'quantum');
		const fps = result.map((d) => d.fingerprint).sort();
		// Both dp-3 (shallow) and dp-4 (deep) must match — the deep one was
		// silently dropped before the fix because Fuse.js penalises late-in-string matches.
		expect(fps).toEqual(['dp-3', 'dp-4']);
	});

	it('matches case-insensitively', () => {
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'GATSBY');
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['dp-1', 'dp-2']);
	});

	it('matches when the user omits the file extension', () => {
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'quantum-physics');
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['dp-3', 'dp-4']);
	});

	it('fuzzy-matches a query with spaces against a hyphenated filename', () => {
		// "great gatsby" (space) vs "great-gatsby" (hyphen) — 1 character difference
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'great gatsby');
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['dp-1', 'dp-2']);
	});

	it('returns nothing for a single-character query (below minMatchCharLength)', () => {
		expect(filterDocuments(deepLibrary, new Set(), new Set(), 'g')).toHaveLength(0);
	});

	it('accepts a 2-character query', () => {
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'ga');
		expect(result.length).toBeGreaterThanOrEqual(1);
	});

	it('fuzzy-tolerates a single typo in the query', () => {
		// "quantun" has one substitution vs "quantum"
		const result = filterDocuments(deepLibrary, new Set(), new Set(), 'quantun');
		expect(result.length).toBeGreaterThanOrEqual(1);
	});

	it('composes correctly with tag filters', () => {
		// "quantum" matches dp-3 and dp-4 (both science); denied: science → none
		const result = filterDocuments(deepLibrary, new Set(), new Set(['science']), 'quantum');
		expect(result).toHaveLength(0);
	});
});

describe('filterDocuments — status filter', () => {
	const lib = [
		makeDoc({ fingerprint: 'a', status: 'Unread' }),
		makeDoc({ fingerprint: 'b', status: 'Reading' }),
		makeDoc({ fingerprint: 'c', status: 'Read' }),
	];

	it('keeps only documents with the given status', () => {
		const result = filterDocuments(lib, new Set(), new Set(), '', undefined, { status: 'Reading' });
		expect(result.map((d) => d.fingerprint)).toEqual(['b']);
	});

	it('returns all when status is null/undefined', () => {
		expect(filterDocuments(lib, new Set(), new Set(), '', undefined, {})).toHaveLength(3);
	});
});

describe('filterDocuments — source filter', () => {
	const lib = [
		makeDoc({ fingerprint: 'a', sourceGuids: { 1: 'a' } }),
		makeDoc({ fingerprint: 'b', sourceGuids: { 2: 'b' } }),
		makeDoc({
			fingerprint: 'c',
			sourceGuids: { 1: 'c' },
			otherFormats: [makeDoc({ fingerprint: 'c2', sourceGuids: { 2: 'c2' } })],
		}),
	];

	it('keeps documents present on the given source (incl. other formats)', () => {
		const result = filterDocuments(lib, new Set(), new Set(), '', undefined, { sourceId: 2 });
		expect(result.map((d) => d.fingerprint).sort()).toEqual(['b', 'c']);
	});
});

describe('filterDocuments — sorting', () => {
	const lib = [
		makeDoc({ fingerprint: 'a', path: '/x/charlie.epub', size: 30, type_: 'epub', status: 'Read' }),
		makeDoc({ fingerprint: 'b', path: '/x/alpha.pdf', size: 10, type_: 'pdf', status: 'Unread' }),
		makeDoc({ fingerprint: 'c', path: '/x/bravo.epub', size: 20, type_: 'epub', status: 'Reading' }),
	];

	it('sorts by filename ascending', () => {
		const result = filterDocuments(lib, new Set(), new Set(), '', undefined, { sortSubject: 'filename' });
		expect(result.map((d) => d.fingerprint)).toEqual(['b', 'c', 'a']);
	});

	it('sorts by size descending', () => {
		const result = filterDocuments(lib, new Set(), new Set(), '', undefined, {
			sortSubject: 'size',
			sortDirection: 'desc',
		});
		expect(result.map((d) => d.fingerprint)).toEqual(['a', 'c', 'b']);
	});

	it('sorts by status ascending (Unread < Reading < Read)', () => {
		const result = filterDocuments(lib, new Set(), new Set(), '', undefined, { sortSubject: 'status' });
		expect(result.map((d) => d.fingerprint)).toEqual(['b', 'c', 'a']);
	});

	it('does not reorder when a search query is present', () => {
		// query path: relevance order, sort ignored — just assert it returns matches
		const result = filterDocuments(lib, new Set(), new Set(), 'alpha', undefined, { sortSubject: 'size' });
		expect(result.map((d) => d.fingerprint)).toContain('b');
	});
});
