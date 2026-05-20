import type { RemoteFile } from './client';

export interface AggregatedFile extends RemoteFile {
	/** GUIDs keyed by source id — the same file can exist on multiple sources. */
	sourceGuids: Record<number, string>;
	/** Other formats of the same document (epub, pdf, mobi variants). Empty for single-format docs. */
	otherFormats: AggregatedFile[];
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
					otherFormats: [],
				});
			}
		}
	}

	return Array.from(byFingerprint.values());
}

const FORMAT_PRIORITY: Record<string, number> = { epub: 0, pdf: 1, mobi: 2 };
const formatPriority = (type_: string) => FORMAT_PRIORITY[type_.toLowerCase()] ?? 3;

/**
 * Collapse files that share a document_guid into a single representative entry.
 * The preferred format (epub > pdf > mobi) becomes the primary; the rest are
 * stored in `otherFormats`. Files without a document_guid are returned unchanged.
 */
export function groupByDocumentGuid(files: AggregatedFile[]): AggregatedFile[] {
	const byGuid = new Map<string, AggregatedFile[]>();
	const ungrouped: AggregatedFile[] = [];

	for (const file of files) {
		if (file.document_guid) {
			const group = byGuid.get(file.document_guid);
			if (group) {
				group.push(file);
			} else {
				byGuid.set(file.document_guid, [file]);
			}
		} else {
			ungrouped.push(file);
		}
	}

	const grouped: AggregatedFile[] = [];
	for (const formats of byGuid.values()) {
		formats.sort((a, b) => formatPriority(a.type_) - formatPriority(b.type_));
		const [primary, ...rest] = formats;
		grouped.push({ ...primary, otherFormats: rest });
	}

	return [...grouped, ...ungrouped];
}
