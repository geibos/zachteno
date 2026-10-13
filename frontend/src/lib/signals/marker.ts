/**
 * Постоянный маркер браузера (design D14): 128 случайных бит, хранится сразу
 * в трёх местах — cookie `zt_m`, localStorage и IndexedDB. Если пользователь
 * очистил часть хранилищ, маркер восстанавливается из уцелевшего и дописывается
 * в остальные.
 */

export const MARKER_COOKIE = 'zt_m';
const MARKER_RE = /^[0-9a-f]{32}$/;

export interface MarkerStore {
	readonly name: string;
	read(): Promise<string | null>;
	write(value: string): Promise<void>;
}

/** 128 бит из криптографического генератора, hex. */
export function generateMarker(random: (bytes: Uint8Array) => Uint8Array = (b) => crypto.getRandomValues(b)): string {
	const bytes = random(new Uint8Array(16));
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function isMarker(value: string | null | undefined): value is string {
	return typeof value === 'string' && MARKER_RE.test(value);
}

/**
 * Возвращает маркер: первый корректный из хранилищ (в порядке приоритета) или новый.
 * Хранилища, где маркера нет или он другой, перезаписываются. Ошибки хранилищ
 * (приватный режим, запрет cookie) не мешают — работаем с теми, что доступны.
 */
export async function ensureMarker(stores: MarkerStore[], generate: () => string = () => generateMarker()): Promise<string> {
	const values = await Promise.all(
		stores.map(async (store) => {
			try {
				return await store.read();
			} catch {
				return null;
			}
		})
	);
	const marker = values.find(isMarker) ?? generate();
	await Promise.all(
		stores.map(async (store, index) => {
			if (values[index] === marker) return;
			try {
				await store.write(marker);
			} catch {
				// хранилище недоступно — пропускаем
			}
		})
	);
	return marker;
}

// --- реализации для браузера ---

const YEAR_SECONDS = 365 * 24 * 60 * 60;

export const cookieStore: MarkerStore = {
	name: 'cookie',
	async read() {
		const match = document.cookie.split('; ').find((part) => part.startsWith(`${MARKER_COOKIE}=`));
		return match ? decodeURIComponent(match.slice(MARKER_COOKIE.length + 1)) : null;
	},
	async write(value) {
		const secure = location.protocol === 'https:' ? '; Secure' : '';
		document.cookie = `${MARKER_COOKIE}=${value}; Max-Age=${YEAR_SECONDS}; Path=/; SameSite=Lax${secure}`;
	}
};

export const localStorageStore: MarkerStore = {
	name: 'localStorage',
	async read() {
		return localStorage.getItem(MARKER_COOKIE);
	},
	async write(value) {
		localStorage.setItem(MARKER_COOKIE, value);
	}
};

const IDB_NAME = 'zachteno-marker';
const IDB_STORE = 'kv';

function openMarkerDb(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const request = indexedDB.open(IDB_NAME, 1);
		request.onupgradeneeded = () => request.result.createObjectStore(IDB_STORE);
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

export const indexedDbStore: MarkerStore = {
	name: 'indexedDB',
	async read() {
		const db = await openMarkerDb();
		return new Promise<string | null>((resolve, reject) => {
			const request = db.transaction(IDB_STORE, 'readonly').objectStore(IDB_STORE).get(MARKER_COOKIE);
			request.onsuccess = () => resolve(typeof request.result === 'string' ? request.result : null);
			request.onerror = () => reject(request.error);
		});
	},
	async write(value) {
		const db = await openMarkerDb();
		await new Promise<void>((resolve, reject) => {
			const tx = db.transaction(IDB_STORE, 'readwrite');
			tx.objectStore(IDB_STORE).put(value, MARKER_COOKIE);
			tx.oncomplete = () => resolve();
			tx.onerror = () => reject(tx.error);
		});
	}
};

export const browserMarkerStores: MarkerStore[] = [cookieStore, localStorageStore, indexedDbStore];
