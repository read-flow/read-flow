import { writable, derived } from 'svelte/store';
import {
	fetchAllFiles,
	fetchDocumentMetaMap,
	type AggregatedFile,
	type DocumentMeta,
} from '$lib/api/aggregator';
import { filterDocuments } from '$lib/utils/filter';

export const allDocuments = writable<AggregatedFile[]>([]);
export const documentMetaMap = writable<Map<string, DocumentMeta>>(new Map());
export const isLoading = writable(false);
export const loadError = writable<string | null>(null);

export const searchQuery = writable('');
export const allowedTags = writable<Set<string>>(new Set());
export const deniedTags = writable<Set<string>>(new Set());

export async function refreshDocuments(): Promise<void> {
	isLoading.set(true);
	loadError.set(null);
	try {
		const [files, metaMap] = await Promise.all([fetchAllFiles(), fetchDocumentMetaMap()]);
		allDocuments.set(files);
		documentMetaMap.set(metaMap);
	} catch (err) {
		loadError.set(err instanceof Error ? err.message : 'Failed to load documents.');
	} finally {
		isLoading.set(false);
	}
}

export const filteredDocuments = derived(
	[allDocuments, searchQuery, allowedTags, deniedTags, documentMetaMap],
	([$all, $query, $allowed, $denied, $metaMap]) =>
		filterDocuments($all, $allowed, $denied, $query, $metaMap),
);

export const allTags = derived(allDocuments, ($all) => {
	const tags = new Set<string>();
	for (const file of $all) file.tags.forEach((t) => tags.add(t));
	return Array.from(tags).sort();
});
