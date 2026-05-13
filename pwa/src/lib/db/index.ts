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

export interface ReadingProgress {
	fingerprint: string;
	progress: string;
	lastUpdated: string;
}

export interface Preference {
	key: string;
	value: string;
}

const db = new Dexie('ReadFlowDB') as Dexie & {
	sources: EntityTable<Source, 'id'>;
	readingProgress: EntityTable<ReadingProgress, 'fingerprint'>;
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

export { db };
