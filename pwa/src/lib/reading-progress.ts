/**
 * Combined reading-position storage shared by the PDF and EPUB readers.
 *
 * @feature: reading.progress
 *
 * Mirrors `cosmic/src/reading_progress.rs`: the same document can be read in
 * either viewer (COSMIC's EPUB/MuPDF viewers, or this PWA's PDF/EPUB
 * readers), and each keeps position in its own format — a CFI for EPUB, a
 * page number for PDF. Both are stored side by side in `ReadingState.position`
 * so switching viewers/apps resumes from that viewer's own last spot instead
 * of clobbering the other one's:
 *
 * ```json
 * {"viewer": "epub", "epub": {"cfi": "..."}, "mupdf": {"page": 42}}
 * ```
 *
 * Rows written before this format existed (or by a reader that only speaks
 * its own raw format) store one viewer's position directly, untagged.
 * `extractPosition` and `mergePosition` recognize those by their distinctive
 * keys (`page` for the PDF/MuPDF viewer; `cfi`/`chapter` for the EPUB
 * viewer) so they migrate into the right slot instead of being misread by
 * the other viewer or silently dropped.
 */

export type Viewer = 'epub' | 'mupdf';

function isPlainObject(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseJson(value: string): unknown {
	try {
		return JSON.parse(value);
	} catch {
		return undefined;
	}
}

/** Which viewer an untagged (pre-combined-format) position belongs to. */
function sniffLegacyViewer(obj: Record<string, unknown>): Viewer | null {
	if ('page' in obj) return 'mupdf';
	if ('cfi' in obj || 'chapter' in obj) return 'epub';
	return null;
}

/**
 * Extract `viewer`'s own position from a stored position string, as a raw
 * JSON string ready to feed into that viewer's own parser. `null` means no
 * saved position for this viewer (it should start from the beginning).
 */
export function extractPosition(stored: string | undefined | null, viewer: Viewer): string | null {
	if (!stored) return null;
	const parsed = parseJson(stored);
	if (!isPlainObject(parsed)) return null;

	if ('viewer' in parsed) {
		const own = parsed[viewer];
		return own === undefined || own === null ? null : JSON.stringify(own);
	}

	// Untagged legacy row: only hand it back if it looks like this viewer's
	// own format, otherwise it belongs to the other viewer.
	return sniffLegacyViewer(parsed) === viewer ? stored : null;
}

/**
 * Merge `ownPosition` (this viewer's own raw position JSON string) into
 * `existing` (the previously-stored combined or legacy position, if any),
 * preserving the other viewer's position untouched. Returns the new
 * combined string to persist.
 */
export function mergePosition(existing: string | undefined | null, viewer: Viewer, ownPosition: string): string {
	let epub: unknown = null;
	let mupdf: unknown = null;

	if (existing) {
		const parsed = parseJson(existing);
		if (isPlainObject(parsed)) {
			if ('viewer' in parsed) {
				epub = parsed.epub ?? null;
				mupdf = parsed.mupdf ?? null;
			} else {
				const legacy = sniffLegacyViewer(parsed);
				if (legacy === 'epub') epub = parsed;
				else if (legacy === 'mupdf') mupdf = parsed;
			}
		}
	}

	const ownValue = parseJson(ownPosition) ?? ownPosition;
	if (viewer === 'epub') epub = ownValue;
	else mupdf = ownValue;

	return JSON.stringify({ viewer, epub, mupdf });
}
