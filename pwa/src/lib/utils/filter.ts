import Fuse from 'fuse.js';
import type { AggregatedFile } from '$lib/api/merge';
import type { DocumentMeta, ReadingStatus } from '$lib/api/client';

export type SortSubject = 'filename' | 'title' | 'size' | 'type' | 'status';
export type SortDirection = 'asc' | 'desc';

export interface ListOptions {
	/** Keep only documents with this reading status. */
	status?: ReadingStatus | null;
	/** Keep only documents that have at least one format of this type. */
	type_?: string | null;
	/** Keep only documents available on this source (by Dexie source id). */
	sourceId?: number | null;
	/** Sort subject (applied only when there is no search query). */
	sortSubject?: SortSubject;
	sortDirection?: SortDirection;
}

function basename(path: string): string {
	return path.split('/').pop() ?? path;
}

/** True when the document (any of its formats) lives on the given source. */
function hasSource(doc: AggregatedFile, sourceId: number): boolean {
	if (doc.sourceGuids[sourceId] !== undefined) return true;
	return doc.otherFormats.some((f) => f.sourceGuids[sourceId] !== undefined);
}

const STATUS_ORDER: Record<ReadingStatus, number> = { Unread: 0, Reading: 1, Read: 2 };

function titleOf(doc: AggregatedFile, metaMap?: Map<string, DocumentMeta>): string {
	const t = doc.document_guid ? metaMap?.get(doc.document_guid)?.title : undefined;
	return t ?? basename(doc.path);
}

function compareDocs(
	a: AggregatedFile,
	b: AggregatedFile,
	subject: SortSubject,
	metaMap?: Map<string, DocumentMeta>,
): number {
	switch (subject) {
		case 'size':
			return a.size - b.size;
		case 'type':
			return a.type_.localeCompare(b.type_);
		case 'status':
			return STATUS_ORDER[a.status] - STATUS_ORDER[b.status];
		case 'title':
			return titleOf(a, metaMap).localeCompare(titleOf(b, metaMap));
		case 'filename':
		default:
			return basename(a.path).localeCompare(basename(b.path));
	}
}

export function filterDocuments(
	all: AggregatedFile[],
	allowed: Set<string>,
	denied: Set<string>,
	query: string,
	metaMap?: Map<string, DocumentMeta>,
	opts: ListOptions = {},
): AggregatedFile[] {
	let results = all;

	if (allowed.size > 0) {
		results = results.filter((f) => [...allowed].every((t) => f.tags.includes(t)));
	}
	if (denied.size > 0) {
		results = results.filter((f) => ![...denied].some((t) => f.tags.includes(t)));
	}
	if (opts.status) {
		results = results.filter((f) => f.status === opts.status);
	}
	if (opts.type_) {
		results = results.filter(
			(f) => f.type_ === opts.type_ || f.otherFormats.some((fmt) => fmt.type_ === opts.type_),
		);
	}
	if (opts.sourceId != null) {
		results = results.filter((f) => hasSource(f, opts.sourceId as number));
	}

	if (query.trim()) {
		const fuse = new Fuse(results, {
			keys: [
				{
					name: 'basename',
					getFn: (doc) => {
						const name = doc.path.split('/').pop() ?? doc.path;
						return name.replace(/\.[^/.]+$/, '');
					},
				},
				{
					name: 'title',
					getFn: (doc) => {
						if (!metaMap || !doc.document_guid) return '';
						return metaMap.get(doc.document_guid)?.title ?? '';
					},
				},
				{
					name: 'authors',
					getFn: (doc) => {
						if (!metaMap || !doc.document_guid) return '';
						return metaMap.get(doc.document_guid)?.authors?.join(' ') ?? '';
					},
				},
			],
			threshold: 0.3,
			includeScore: true,
			minMatchCharLength: 2,
		});
		// Search results are returned in relevance order (sort is not applied).
		return fuse.search(query.trim()).map((r) => r.item);
	}

	if (opts.sortSubject) {
		const dir = opts.sortDirection === 'desc' ? -1 : 1;
		results = [...results].sort((a, b) => dir * compareDocs(a, b, opts.sortSubject as SortSubject, metaMap));
	}

	return results;
}
