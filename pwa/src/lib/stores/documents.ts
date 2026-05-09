import { writable, derived } from 'svelte/store';
import Fuse from 'fuse.js';
import { fetchAllFiles, type AggregatedFile } from '$lib/api/aggregator';

export const allDocuments = writable<AggregatedFile[]>([]);
export const isLoading = writable(false);
export const loadError = writable<string | null>(null);

export const searchQuery = writable('');
export const allowedTags = writable<Set<string>>(new Set());
export const deniedTags = writable<Set<string>>(new Set());

export async function refreshDocuments(): Promise<void> {
	isLoading.set(true);
	loadError.set(null);
	try {
		const files = await fetchAllFiles();
		allDocuments.set(files);
	} catch (err) {
		loadError.set(err instanceof Error ? err.message : 'Failed to load documents.');
	} finally {
		isLoading.set(false);
	}
}

export const filteredDocuments = derived(
	[allDocuments, searchQuery, allowedTags, deniedTags],
	([$all, $query, $allowed, $denied]) => {
		let results = $all;

		// Tag filtering (allow = AND, deny = NOT)
		if ($allowed.size > 0) {
			results = results.filter((f) => [...$allowed].every((t) => f.tags.includes(t)));
		}
		if ($denied.size > 0) {
			results = results.filter((f) => ![...$denied].some((t) => f.tags.includes(t)));
		}

		// Fuzzy search
		if ($query.trim()) {
			const fuse = new Fuse(results, {
				keys: ['path'],
				threshold: 0.3,
				includeScore: true,
				minMatchCharLength: 2,
			});
			results = fuse.search($query.trim()).map((r) => r.item);
		}

		return results;
	},
);

export const allTags = derived(allDocuments, ($all) => {
	const tags = new Set<string>();
	for (const file of $all) file.tags.forEach((t) => tags.add(t));
	return Array.from(tags).sort();
});
