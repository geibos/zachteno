import { errorMessage } from '$lib/api/client';

/**
 * Загрузка данных страницы с состояниями «загрузка / ошибка / данные».
 * `reload()` не сбрасывает уже показанные данные — экран не мигает.
 */
export class Resource<T> {
	data = $state<T | null>(null);
	error = $state<string | null>(null);
	loading = $state(false);
	/** Код ошибки HTTP последней неудачной загрузки (404 → «нет доступа»). */
	status = $state<number | null>(null);

	readonly #fetcher: () => Promise<T>;
	#generation = 0;

	constructor(fetcher: () => Promise<T>) {
		this.#fetcher = fetcher;
	}

	async reload(): Promise<void> {
		const generation = ++this.#generation;
		this.loading = true;
		try {
			const data = await this.#fetcher();
			if (generation !== this.#generation) return;
			this.data = data;
			this.error = null;
			this.status = null;
		} catch (error) {
			if (generation !== this.#generation) return;
			this.error = errorMessage(error);
			this.status =
				typeof error === 'object' && error !== null && 'status' in error
					? Number((error as { status: unknown }).status)
					: null;
		} finally {
			if (generation === this.#generation) this.loading = false;
		}
	}
}

/** Создаёт ресурс и сразу запускает загрузку. */
export function resource<T>(fetcher: () => Promise<T>): Resource<T> {
	const res = new Resource(fetcher);
	void res.reload();
	return res;
}
