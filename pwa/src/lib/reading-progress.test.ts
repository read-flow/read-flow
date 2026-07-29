import { describe, expect, it } from 'vitest';
import { extractPosition, mergePosition } from './reading-progress';

describe('extractPosition / mergePosition', () => {
	it('round-trips its own position through merge then extract', () => {
		const stored = mergePosition(null, 'epub', '{"cfi":"epubcfi(/6/4)"}');
		expect(extractPosition(stored, 'epub')).toBe('{"cfi":"epubcfi(/6/4)"}');
	});

	it('preserves the other viewer position across merges', () => {
		const stored1 = mergePosition(null, 'epub', '{"cfi":"epubcfi(/6/4)"}');
		const stored2 = mergePosition(stored1, 'mupdf', '{"page":42}');

		expect(extractPosition(stored2, 'epub')).toBe('{"cfi":"epubcfi(/6/4)"}');
		expect(extractPosition(stored2, 'mupdf')).toBe('{"page":42}');
	});

	it('keeps both positions current when switching back and forth', () => {
		let stored = mergePosition(null, 'epub', '{"cfi":"a"}');
		stored = mergePosition(stored, 'mupdf', '{"page":1}');
		stored = mergePosition(stored, 'epub', '{"cfi":"b"}');
		stored = mergePosition(stored, 'mupdf', '{"page":2}');

		expect(extractPosition(stored, 'epub')).toBe('{"cfi":"b"}');
		expect(extractPosition(stored, 'mupdf')).toBe('{"page":2}');
	});

	it('returns null when the viewer never saved a position', () => {
		const stored = mergePosition(null, 'epub', '{"cfi":"a"}');
		expect(extractPosition(stored, 'mupdf')).toBeNull();
	});

	it('returns null for absent or garbage input', () => {
		expect(extractPosition('', 'epub')).toBeNull();
		expect(extractPosition(undefined, 'epub')).toBeNull();
		expect(extractPosition('not json', 'mupdf')).toBeNull();
	});

	it('migrates a legacy untagged mupdf position into the mupdf slot', () => {
		const legacy = '{"page":7}';
		expect(extractPosition(legacy, 'mupdf')).toBe(legacy);
		expect(extractPosition(legacy, 'epub')).toBeNull();

		const stored = mergePosition(legacy, 'epub', '{"cfi":"a"}');
		expect(extractPosition(stored, 'mupdf')).toBe(legacy);
		expect(extractPosition(stored, 'epub')).toBe('{"cfi":"a"}');
	});

	it('migrates a legacy untagged epub cfi position into the epub slot', () => {
		const legacy = '{"cfi":"epubcfi(/6/4)"}';
		expect(extractPosition(legacy, 'epub')).toBe(legacy);
		expect(extractPosition(legacy, 'mupdf')).toBeNull();

		const stored = mergePosition(legacy, 'mupdf', '{"page":3}');
		expect(extractPosition(stored, 'epub')).toBe(legacy);
		expect(extractPosition(stored, 'mupdf')).toBe('{"page":3}');
	});

	it('migrates a legacy untagged epub chapter position into the epub slot', () => {
		const legacy = '{"chapter":2,"block":5}';
		expect(extractPosition(legacy, 'epub')).toBe(legacy);
		expect(extractPosition(legacy, 'mupdf')).toBeNull();
	});

	// Regression: COSMIC's MuPDF viewer always saves the combined/tagged
	// format (see cosmic/src/reading_progress.rs). Before this module
	// existed, the PWA's PDF reader parsed `saved.position` directly and
	// looked for a top-level "page" key, which a tagged position never has
	// (it's nested under "mupdf") — so progress written by COSMIC's PDF
	// viewer silently failed to resume in the PWA, always restarting at
	// page 1.
	it('extracts a PDF page position saved by COSMIC in the tagged format', () => {
		const cosmicSaved = JSON.stringify({ viewer: 'mupdf', epub: null, mupdf: { page: 42 } });
		expect(extractPosition(cosmicSaved, 'mupdf')).toBe('{"page":42}');
	});

	// Same regression for the EPUB reader, on the "epub" slot.
	it('extracts an EPUB cfi position saved by COSMIC in the tagged format', () => {
		const cosmicSaved = JSON.stringify({
			viewer: 'epub',
			epub: { cfi: 'epubcfi(/6/4)' },
			mupdf: null,
		});
		expect(extractPosition(cosmicSaved, 'epub')).toBe('{"cfi":"epubcfi(/6/4)"}');
	});

	it('saving from the PWA after COSMIC preserves the other viewer slot', () => {
		const cosmicSaved = JSON.stringify({ viewer: 'mupdf', epub: null, mupdf: { page: 42 } });
		const stored = mergePosition(cosmicSaved, 'mupdf', '{"page":50}');
		expect(extractPosition(stored, 'mupdf')).toBe('{"page":50}');
	});
});
