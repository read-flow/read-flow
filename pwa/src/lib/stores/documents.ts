import { writable, derived } from 'svelte/store';
import {
	fetchAllFiles,
	fetchDocumentMetaMap,
	type AggregatedFile,
	type DocumentMeta,
} from '$lib/api/aggregator';
import { filterDocuments, type SortSubject, type SortDirection } from '$lib/utils/filter';
import type { ReadingStatus } from '$lib/api/client';

/**
 * Find an AggregatedFile by fingerprint, searching both top-level entries and
 * their otherFormats. Necessary because groupByDocumentGuid only keeps one
 * format as the primary entry; all others live in otherFormats.
 */
export function findByFingerprint(docs: AggregatedFile[], fingerprint: string): AggregatedFile | null {
	return (
		docs.find((d) => d.fingerprint === fingerprint) ??
		docs.flatMap((d) => d.otherFormats).find((d) => d.fingerprint === fingerprint) ??
		null
	);
}

export const allDocuments = writable<AggregatedFile[]>([]);
export const documentMetaMap = writable<Map<string, DocumentMeta>>(new Map());
export const isLoading = writable(false);
export const loadError = writable<string | null>(null);

// @feature: documents.search
export const searchQuery = writable('');
// @feature: documents.filter_by_tag
export const allowedTags = writable<Set<string>>(new Set());
export const deniedTags = writable<Set<string>>(new Set());
// @feature: documents.filter_by_status
export const statusFilter = writable<ReadingStatus | null>(null);
// @feature: documents.filter_by_type
export const typeFilter = writable<string | null>(null);
// @feature: documents.filter_by_source
export const sourceFilter = writable<number | null>(null);
// @feature: documents.sort
export const sortSubject = writable<SortSubject>('filename');
export const sortDirection = writable<SortDirection>('asc');

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
	[
		allDocuments,
		searchQuery,
		allowedTags,
		deniedTags,
		documentMetaMap,
		statusFilter,
		typeFilter,
		sourceFilter,
		sortSubject,
		sortDirection,
	],
	([$all, $query, $allowed, $denied, $metaMap, $status, $type, $source, $sortSubject, $sortDirection]) =>
		filterDocuments($all, $allowed, $denied, $query, $metaMap, {
			status: $status,
			type_: $type,
			sourceId: $source,
			sortSubject: $sortSubject,
			sortDirection: $sortDirection,
		}),
);

export const allTags = derived(allDocuments, ($all) => {
	const tags = new Set<string>();
	for (const file of $all) file.tags.forEach((t) => tags.add(t));
	return Array.from(tags).sort();
});
