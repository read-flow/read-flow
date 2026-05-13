import Fuse from 'fuse.js';
import type { AggregatedFile } from '$lib/api/merge';

export function filterDocuments(
	all: AggregatedFile[],
	allowed: Set<string>,
	denied: Set<string>,
	query: string,
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
			],
			threshold: 0.3,
			includeScore: true,
			minMatchCharLength: 2,
		});
		results = fuse.search(query.trim()).map((r) => r.item);
	}

	return results;
}
