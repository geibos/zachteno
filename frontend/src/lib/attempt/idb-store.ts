import { MemoryStore, type PendingAnswer, type PendingStore } from './save-queue';

const DB_NAME = 'zachteno';
const STORE = 'pending-answers';

function promisify<T>(request: IDBRequest<T>): Promise<T> {
	return new Promise((resolve, reject) => {
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

/** Несохранённые ответы в IndexedDB: переживают перезагрузку вкладки без сети. */
class IndexedDbStore implements PendingStore {
	readonly #db: Promise<IDBDatabase>;

	constructor() {
		this.#db = new Promise((resolve, reject) => {
			const open = indexedDB.open(DB_NAME, 1);
			open.onupgradeneeded = () => {
				const store = open.result.createObjectStore(STORE, { keyPath: ['attemptId', 'questionId'] });
				store.createIndex('attemptId', 'attemptId');
			};
			open.onsuccess = () => resolve(open.result);
			open.onerror = () => reject(open.error);
		});
	}

	async #tx(mode: IDBTransactionMode): Promise<IDBObjectStore> {
		const db = await this.#db;
		return db.transaction(STORE, mode).objectStore(STORE);
	}

	async load(attemptId: string): Promise<PendingAnswer[]> {
		const store = await this.#tx('readonly');
		return promisify(store.index('attemptId').getAll(attemptId) as IDBRequest<PendingAnswer[]>);
	}

	async put(entry: PendingAnswer): Promise<void> {
		const store = await this.#tx('readwrite');
		const current = (await promisify(store.get([entry.attemptId, entry.questionId]))) as
			| PendingAnswer
			| undefined;
		if (current && current.client_seq > entry.client_seq) return;
		await promisify(store.put({ ...entry, selected_option_ids: [...entry.selected_option_ids] }));
	}

	async remove(attemptId: string, questionId: string, clientSeq: number): Promise<void> {
		const store = await this.#tx('readwrite');
		const current = (await promisify(store.get([attemptId, questionId]))) as PendingAnswer | undefined;
		if (current && current.client_seq <= clientSeq) await promisify(store.delete([attemptId, questionId]));
	}
}

/** IndexedDB, а если она недоступна (приватный режим и т.п.) — память. */
export function createPendingStore(): PendingStore {
	try {
		if (typeof indexedDB !== 'undefined') return new IndexedDbStore();
	} catch {
		// упадём на память ниже
	}
	return new MemoryStore();
}
