import Fuse from 'fuse.js';
import type { AggregatedFile } from '$lib/api/merge';
import type { DocumentMeta } from '$lib/api/client';

export function filterDocuments(
	all: AggregatedFile[],
	allowed: Set<string>,
	denied: Set<string>,
	query: string,
	metaMap?: Map<string, DocumentMeta>,
): AggregatedFile[] {
	let results = all;

	if (allowed.size > 0) {
		results = results.filter((f) => [...allowed].every((t) => f.tags.includes(t)));
	}
	if (denied.size > 0) {
		results = results.filter((f) => ![...denied].some((t) => f.tags.includes(t)));
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
		results = fuse.search(query.trim()).map((r) => r.item);
	}

	return results;
}
