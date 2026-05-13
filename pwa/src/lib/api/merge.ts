import type { RemoteFile } from './client';

export interface AggregatedFile extends RemoteFile {
	/** GUIDs keyed by source id — the same file can exist on multiple sources. */
	sourceGuids: Record<number, string>;
}

/**
 * Merge file listings from multiple sources into one deduplicated list.
 * Files are matched by fingerprint; tags from all sources are unioned.
 */
export function mergeFiles(
	batches: Array<{ sourceId: number; files: RemoteFile[] }>,
): AggregatedFile[] {
	const byFingerprint = new Map<string, AggregatedFile>();

	for (const { sourceId, files } of batches) {
		for (const file of files) {
			const existing = byFingerprint.get(file.fingerprint);
			if (existing) {
				existing.sourceGuids[sourceId] = file.guid;
				for (const tag of file.tags) {
					if (!existing.tags.includes(tag)) existing.tags.push(tag);
				}
			} else {
				byFingerprint.set(file.fingerprint, {
					...file,
					sourceGuids: { [sourceId]: file.guid },
				});
			}
		}
	}

	return Array.from(byFingerprint.values());
}
