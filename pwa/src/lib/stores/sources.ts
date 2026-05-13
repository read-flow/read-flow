import { writable } from 'svelte/store';
import { db, type Source } from '$lib/db';
import { ReadFlowClient } from '$lib/api/client';

export const sources = writable<Source[]>([]);

export async function loadSources(): Promise<void> {
	const all = await db.sources.orderBy('order').toArray();
	sources.set(all);
}

export async function addSource(
	data: Omit<Source, 'id' | 'order'>,
): Promise<{ ok: true } | { ok: false; error: string }> {
	try {
		// Test connectivity before persisting
		const client = new ReadFlowClient({ ...data, id: undefined, order: 0 });
		await client.status();
	} catch {
		return { ok: false, error: 'Could not connect to the server. Check the URL and credentials.' };
	}

	const count = await db.sources.count();
	await db.sources.add({ ...data, order: count });
	await loadSources();
	return { ok: true };
}

export async function removeSource(id: number): Promise<void> {
	await db.sources.delete(id);
	// Re-normalise order values
	const remaining = await db.sources.orderBy('order').toArray();
	await Promise.all(remaining.map((s, i) => db.sources.update(s.id!, { order: i })));
	await loadSources();
}

export async function updateSource(id: number, data: Partial<Omit<Source, 'id' | 'order'>>): Promise<void> {
	await db.sources.update(id, data);
	await loadSources();
}

export async function moveSource(id: number, direction: 'up' | 'down'): Promise<void> {
	const all = await db.sources.orderBy('order').toArray();
	const idx = all.findIndex((s) => s.id === id);
	if (idx === -1) return;

	const swapIdx = direction === 'up' ? idx - 1 : idx + 1;
	if (swapIdx < 0 || swapIdx >= all.length) return;

	const a = all[idx];
	const b = all[swapIdx];
	await db.sources.update(a.id!, { order: b.order });
	await db.sources.update(b.id!, { order: a.order });
	await loadSources();
}
