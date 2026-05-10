import { db } from '$lib/db';
import { ReadFlowClient, type RemoteFile, type RemoteReadingProgress } from './client';
import { mergeFiles, type AggregatedFile } from './merge';

export type { AggregatedFile } from './merge';

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

	return mergeFiles(batches);
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

export async function addTagsToFile(fingerprint: string, tags: string[]): Promise<void> {
	const clients = await getClients();
	// Fan out to all sources that have the file
	await Promise.allSettled(
		clients.map(async ({ client }) => {
			const files = await client.getFiles();
			const file = files.find((f) => f.fingerprint === fingerprint);
			if (file) await client.addTags(file.guid, tags);
		}),
	);
}

export async function removeTagsFromFile(fingerprint: string, tags: string[]): Promise<void> {
	const clients = await getClients();
	await Promise.allSettled(
		clients.map(async ({ client }) => {
			const files = await client.getFiles();
			const file = files.find((f) => f.fingerprint === fingerprint);
			if (file) await client.deleteTags(file.guid, tags);
		}),
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
