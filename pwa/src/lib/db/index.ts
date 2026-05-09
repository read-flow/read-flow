import Dexie, { type EntityTable } from 'dexie';

export interface Source {
	id?: number;
	name: string;
	baseUrl: string;
	userId: string;
	passphrase: string;
	order: number;
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

export { db };
