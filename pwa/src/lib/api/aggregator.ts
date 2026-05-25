import { db } from '$lib/db';
import {
	ReadFlowClient,
	type RemoteFile,
	type RemoteReadingProgress,
	type RemoteDocument,
	type DocumentMeta,
} from './client';
import { mergeFiles, groupByDocumentGuid, type AggregatedFile } from './merge';

export type { AggregatedFile } from './merge';
export type { RemoteDocument, DocumentMeta } from './client';

async function getClients(): Promise<Array<{ id: number; client: ReadFlowClient }>> {
	const sources = await db.sources.orderBy('order').toArray();
	return sources
		.filter((s) => s.id !== undefined)
		.map((s) => ({ id: s.id as number, client: new ReadFlowClient(s) }));
}

export async function fetchAllFiles(): Promise<AggregatedFile[]> {
	const clients = await getClients();
	if (clients.length === 0) return [];

	const results = await Promise.allSettled(
		clients.map(async ({ id, client }) => ({ sourceId: id, files: await client.getFiles() })),
	);

	const batches = results
		.filter((r): r is PromiseFulfilledResult<{ sourceId: number; files: RemoteFile[] }> => r.status === 'fulfilled')
		.map((r) => r.value);

	return groupByDocumentGuid(mergeFiles(batches));
}

export async function fetchAllTags(): Promise<string[]> {
	const clients = await getClients();
	const results = await Promise.allSettled(clients.map(({ client }) => client.getAllTags()));
	const tags = new Set<string>();
	for (const result of results) {
		if (result.status === 'fulfilled') result.value.forEach((t) => tags.add(t));
	}
	return Array.from(tags).sort();
}

export async function addTagsToFile(sourceGuids: Record<number, string>, tags: string[]): Promise<void> {
	const sources = await db.sources.orderBy('order').toArray();
	await Promise.allSettled(
		sources
			.filter((s) => s.id !== undefined && sourceGuids[s.id as number] !== undefined)
			.map((s) => new ReadFlowClient(s).addTags(sourceGuids[s.id as number], tags)),
	);
}

export async function removeTagsFromFile(sourceGuids: Record<number, string>, tags: string[]): Promise<void> {
	const sources = await db.sources.orderBy('order').toArray();
	await Promise.allSettled(
		sources
			.filter((s) => s.id !== undefined && sourceGuids[s.id as number] !== undefined)
			.map((s) => new ReadFlowClient(s).deleteTags(sourceGuids[s.id as number], tags)),
	);
}

export async function fetchReadingProgress(fingerprint: string): Promise<RemoteReadingProgress | null> {
	// Check local IndexedDB first — instant and works offline
	const local = await db.readingProgress.get(fingerprint);

	const clients = await getClients();
	const results = await Promise.allSettled(
		clients.map(({ client }) => client.getReadingProgress(fingerprint)),
	);

	let newest: RemoteReadingProgress | null = local
		? { fingerprint: local.fingerprint, progress: local.progress, last_updated: local.lastUpdated }
		: null;

	for (const result of results) {
		if (result.status !== 'fulfilled' || result.value === null) continue;
		if (!newest || result.value.last_updated > newest.last_updated) {
			newest = result.value;
		}
	}
	return newest;
}

export async function saveReadingProgress(progress: RemoteReadingProgress): Promise<void> {
	// Write to local DB immediately — survives offline and is instant
	await db.readingProgress.put({
		fingerprint: progress.fingerprint,
		progress: progress.progress,
		lastUpdated: progress.last_updated,
	});

	// Fan out to remote sources; failures don't block the reader
	const clients = await getClients();
	const results = await Promise.allSettled(
		clients.map(({ client }) => client.upsertReadingProgress(progress)),
	);
	for (const result of results) {
		if (result.status === 'rejected') {
			console.warn('Failed to save reading progress to a source:', result.reason);
		}
	}
}

/**
 * Fetch document metadata (user-edited) from all sources and return a map
 * of document GUID → DocumentMeta, preferring the first successful source.
 */
export async function fetchDocumentMetaMap(): Promise<Map<string, DocumentMeta>> {
	const clients = await getClients();
	const results = await Promise.allSettled(
		clients.map(({ client }) => client.getDocuments()),
	);

	const map = new Map<string, DocumentMeta>();
	for (const result of results) {
		if (result.status !== 'fulfilled') continue;
		for (const doc of result.value) {
			if (!map.has(doc.guid)) {
				map.set(doc.guid, doc.metadata);
			}
		}
	}
	return map;
}

/**
 * Ensure a document record exists for the given file GUID on a single source client.
 * Returns the document GUID.
 */
async function ensureDocumentOnSource(
	client: ReadFlowClient,
	fileGuid: string,
): Promise<string> {
	const doc = await client.ensureDocumentForFile(fileGuid);
	return doc.guid;
}

/**
 * Update document metadata on all sources that hold the document.
 *
 * If `documentGuid` is null (no document record exists yet for the file),
 * this first calls `ensureDocumentForFile` on each source to create the record,
 * then updates the metadata using the resulting GUID.
 *
 * `sourceGuids` is required when `documentGuid` is null — it maps source IDs to
 * file GUIDs so we know which file to link on each source.
 */
export async function updateDocumentMetadata(
	documentGuid: string | null,
	meta: DocumentMeta,
	sourceGuids?: Record<number, string>,
): Promise<void> {
	const sources = await db.sources.orderBy('order').toArray();

	if (documentGuid) {
		// Fast path: document record already exists.
		await Promise.allSettled(
			sources.map((s) => new ReadFlowClient(s).updateDocumentMetadata(documentGuid, meta)),
		);
	} else {
		// Slow path: ensure a document record per source, then update metadata.
		if (!sourceGuids) return;
		await Promise.allSettled(
			sources
				.filter((s) => s.id !== undefined && sourceGuids[s.id as number] !== undefined)
				.map(async (s) => {
					const client = new ReadFlowClient(s);
					const fileGuid = sourceGuids[s.id as number];
					const docGuid = await ensureDocumentOnSource(client, fileGuid);
					await client.updateDocumentMetadata(docGuid, meta);
				}),
		);
	}
}

/**
 * Merge `losers` into `winner` on every source that holds any of them.
 *
 * For each source:
 * - Resolves winner's document GUID (creating a document record if needed).
 * - Resolves each loser's document GUID (creating if needed).
 * - Calls POST /documents/merge on that source.
 */
export async function mergeDocuments(
	winner: AggregatedFile,
	losers: AggregatedFile[],
): Promise<void> {
	const sources = await db.sources.orderBy('order').toArray();
	await Promise.allSettled(
		sources.map(async (s) => {
			if (s.id === undefined) return;
			const sid = s.id as number;
			const client = new ReadFlowClient(s);

			let winnerGuid = winner.document_guid;
			if (!winnerGuid) {
				const fg = winner.sourceGuids[sid];
				if (!fg) return;
				winnerGuid = await ensureDocumentOnSource(client, fg);
			}

			const loserGuids: string[] = [];
			for (const loser of losers) {
				let loserGuid = loser.document_guid;
				if (!loserGuid) {
					const fg = loser.sourceGuids[sid];
					if (fg) loserGuid = await ensureDocumentOnSource(client, fg);
				}
				if (loserGuid) loserGuids.push(loserGuid);
			}

			if (loserGuids.length > 0) {
				await client.mergeDocuments(winnerGuid, loserGuids);
			}
		}),
	);
}

/**
 * Download a file by trying each source that holds it in order.
 * `sourceGuids` comes from AggregatedFile.sourceGuids (sourceId → GUID).
 */
export async function downloadFileFromSources(
	sourceGuids: Record<number, string>,
	fileName: string,
): Promise<Blob> {
	const sources = await db.sources.orderBy('order').toArray();
	const errors: Error[] = [];

	for (const source of sources) {
		if (source.id === undefined) continue;
		const guid = sourceGuids[source.id];
		if (!guid) continue;
		try {
			return await new ReadFlowClient(source).downloadFile(guid, fileName);
		} catch (err) {
			errors.push(err instanceof Error ? err : new Error(String(err)));
		}
	}

	throw new Error(
		`Could not download "${fileName}" from any source: ${errors.map((e) => e.message).join('; ')}`,
	);
}
