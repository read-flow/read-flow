import Dexie, { type EntityTable } from 'dexie';

export interface Source {
	id?: number;
	name: string;
	baseUrl: string;
	userId: string;
	passphrase: string;
	order: number;
	privateMode?: boolean;
}

export interface ReadingState {
	fingerprint: string;
	status: 'Unread' | 'Reading' | 'Read';
	position: string;
	percentage: number;
	lastUpdated: string;
	statusUpdatedAt: string;
}

export interface Preference {
	key: string;
	value: string;
}

const db = new Dexie('ReadFlowDB') as Dexie & {
	sources: EntityTable<Source, 'id'>;
	readingState: EntityTable<ReadingState, 'fingerprint'>;
	preferences: EntityTable<Preference, 'key'>;
};

db.version(1).stores({
	sources: '++id, order',
	readingProgress: 'fingerprint',
	preferences: 'key',
});

db.version(2)
	.stores({
		sources: '++id, order',
		readingProgress: 'fingerprint',
		preferences: 'key',
	})
	.upgrade((tx) => {
		return tx
			.table('sources')
			.toCollection()
			.modify((source) => {
				source.privateMode = false;
			});
	});

db.version(3)
	.stores({
		sources: '++id, order',
		readingProgress: null,
		readingState: 'fingerprint',
		preferences: 'key',
	})
	.upgrade(async (tx) => {
		const old = await tx.table('readingProgress').toArray();
		const migrated: ReadingState[] = old.map((row) => ({
			fingerprint: row.fingerprint,
			status: 'Unread' as const,
			position: row.progress ?? '{}',
			percentage: 0,
			lastUpdated: row.lastUpdated ?? '1970-01-01T00:00:00Z',
			statusUpdatedAt: '1970-01-01T00:00:00Z',
		}));
		if (migrated.length > 0) {
			await tx.table('readingState').bulkAdd(migrated);
		}
	});

export { db };
