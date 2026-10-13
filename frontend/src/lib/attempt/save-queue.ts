/**
 * Очередь сохранения ответов попытки (spec test-taking, «Автосохранение»; D11).
 *
 * - Каждое изменение получает монотонный `client_seq`; сервер игнорирует старые,
 *   поэтому порядок доставки не важен.
 * - Последнее несохранённое изменение по каждому вопросу лежит в хранилище
 *   (IndexedDB в браузере) и переживает перезагрузку и обрыв связи.
 * - Сетевые ошибки → повтор с нарастающей паузой и сразу при событии `online`.
 * - 409 (время вышло / попытка сдана / вопрос закрыт) → повторов нет, сообщаем наверх.
 */

export interface AnswerPayload {
	selected_option_ids: string[];
	text: string | null;
}

export interface PendingAnswer extends AnswerPayload {
	attemptId: string;
	questionId: string;
	client_seq: number;
}

export type SaveStatus = 'saved' | 'saving' | 'unsaved' | 'error';

export interface PendingStore {
	load(attemptId: string): Promise<PendingAnswer[]>;
	put(entry: PendingAnswer): Promise<void>;
	/** Удаляет запись, если в хранилище нет более нового изменения. */
	remove(attemptId: string, questionId: string, clientSeq: number): Promise<void>;
}

/** Результат отправки. `retry` — сетевой сбой, `conflict` — сохранять больше нельзя. */
export type SendResult =
	| { kind: 'ok' }
	| { kind: 'retry' }
	| { kind: 'conflict'; message: string }
	| { kind: 'rejected'; message: string };

export interface SaveQueueOptions {
	attemptId: string;
	store: PendingStore;
	send: (entry: PendingAnswer) => Promise<SendResult>;
	onStatus: (questionId: string, status: SaveStatus, message?: string) => void;
	onConflict?: (message: string) => void;
	now?: () => number;
	isOnline?: () => boolean;
	/** Паузы между повторами, мс. */
	retryDelays?: number[];
	setTimer?: (fn: () => void, ms: number) => unknown;
	clearTimer?: (handle: unknown) => void;
}

/** Монотонный счётчик на основе времени: не меньше `Date.now()` и строго растёт. */
export class SeqClock {
	#last: number;
	readonly #now: () => number;

	constructor(now: () => number = Date.now, start = 0) {
		this.#now = now;
		this.#last = start;
	}

	next(): number {
		this.#last = Math.max(this.#now(), this.#last + 1);
		return this.#last;
	}

	/** Не выдавать номера меньше уже использованных (например, восстановленных из хранилища). */
	observe(seq: number): void {
		if (seq > this.#last) this.#last = seq;
	}
}

export class SaveQueue {
	readonly #o: Required<Omit<SaveQueueOptions, 'onConflict'>> & Pick<SaveQueueOptions, 'onConflict'>;
	readonly #seq: SeqClock;
	/** Последнее изменение по вопросу, ещё не подтверждённое сервером. */
	readonly #pending = new Map<string, PendingAnswer>();
	readonly #inFlight = new Set<string>();
	readonly #debounce = new Map<string, unknown>();
	readonly #status = new Map<string, SaveStatus>();
	#retryTimer: unknown = null;
	#retryStep = 0;
	#stopped = false;

	constructor(options: SaveQueueOptions) {
		this.#o = {
			now: Date.now,
			isOnline: () => (typeof navigator === 'undefined' ? true : navigator.onLine),
			retryDelays: [1000, 2000, 5000, 10000, 20000],
			setTimer: (fn, ms) => setTimeout(fn, ms),
			clearTimer: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
			...options
		};
		this.#seq = new SeqClock(this.#o.now);
	}

	/**
	 * Учитывает номера, уже сохранённые на сервере, и поднимает из хранилища
	 * несохранённые изменения новее серверных. Возвращает их — интерфейс должен
	 * показать именно эти значения.
	 */
	async restore(server: Array<{ question_id: string; client_seq: number }> = []): Promise<PendingAnswer[]> {
		const serverSeq = new Map<string, number>();
		for (const answer of server) {
			this.#seq.observe(answer.client_seq);
			serverSeq.set(answer.question_id, answer.client_seq);
		}
		let stored: PendingAnswer[] = [];
		try {
			stored = await this.#o.store.load(this.#o.attemptId);
		} catch {
			stored = [];
		}
		const restored: PendingAnswer[] = [];
		for (const entry of stored) {
			this.#seq.observe(entry.client_seq);
			if (entry.client_seq <= (serverSeq.get(entry.questionId) ?? -1)) {
				void this.#o.store.remove(entry.attemptId, entry.questionId, entry.client_seq).catch(() => undefined);
				continue;
			}
			const current = this.#pending.get(entry.questionId);
			if (!current || current.client_seq < entry.client_seq) this.#pending.set(entry.questionId, entry);
		}
		for (const [questionId, entry] of this.#pending) {
			restored.push(entry);
			this.#setStatus(questionId, 'unsaved');
			void this.#flush(questionId);
		}
		return restored;
	}

	/** Новое значение ответа. `debounceMs` — для текста. */
	set(questionId: string, answer: AnswerPayload, debounceMs = 0): PendingAnswer {
		const entry: PendingAnswer = {
			attemptId: this.#o.attemptId,
			questionId,
			selected_option_ids: [...answer.selected_option_ids],
			text: answer.text,
			client_seq: this.#seq.next()
		};
		this.#pending.set(questionId, entry);
		this.#setStatus(questionId, 'unsaved');
		void this.#o.store.put(entry).catch(() => undefined);

		const previous = this.#debounce.get(questionId);
		if (previous !== undefined) this.#o.clearTimer(previous);
		if (debounceMs > 0) {
			this.#debounce.set(
				questionId,
				this.#o.setTimer(() => {
					this.#debounce.delete(questionId);
					void this.#flush(questionId);
				}, debounceMs)
			);
		} else {
			this.#debounce.delete(questionId);
			void this.#flush(questionId);
		}
		return entry;
	}

	/** Отправить всё отложенное немедленно (перед переходом, сдачей, при `online`). */
	async flushAll(): Promise<void> {
		for (const [questionId, handle] of this.#debounce) {
			this.#o.clearTimer(handle);
			this.#debounce.delete(questionId);
		}
		this.#retryStep = 0;
		await Promise.all([...this.#pending.keys()].map((questionId) => this.#flush(questionId)));
	}

	/** Ждёт, пока по вопросу не останется несохранённого (или сохранить не удастся). */
	async settle(questionId: string): Promise<boolean> {
		const handle = this.#debounce.get(questionId);
		if (handle !== undefined) {
			this.#o.clearTimer(handle);
			this.#debounce.delete(questionId);
		}
		await this.#flush(questionId);
		return !this.#pending.has(questionId);
	}

	get unsavedCount(): number {
		return this.#pending.size;
	}

	hasPending(questionId: string): boolean {
		return this.#pending.has(questionId);
	}

	status(questionId: string): SaveStatus | undefined {
		return this.#status.get(questionId);
	}

	/** Остановить повторы (попытка сдана). */
	stop(): void {
		this.#stopped = true;
		if (this.#retryTimer !== null) this.#o.clearTimer(this.#retryTimer);
		for (const handle of this.#debounce.values()) this.#o.clearTimer(handle);
		this.#debounce.clear();
	}

	async #flush(questionId: string): Promise<void> {
		if (this.#stopped || this.#inFlight.has(questionId)) return;
		const entry = this.#pending.get(questionId);
		if (!entry) return;
		if (!this.#o.isOnline()) {
			this.#setStatus(questionId, 'unsaved');
			this.#scheduleRetry();
			return;
		}
		this.#inFlight.add(questionId);
		this.#setStatus(questionId, 'saving');
		let result: SendResult;
		try {
			result = await this.#o.send(entry);
		} catch {
			result = { kind: 'retry' };
		}
		this.#inFlight.delete(questionId);

		const latest = this.#pending.get(questionId);
		const superseded = latest !== undefined && latest.client_seq !== entry.client_seq;

		switch (result.kind) {
			case 'ok':
				this.#retryStep = 0;
				void this.#o.store.remove(entry.attemptId, questionId, entry.client_seq).catch(() => undefined);
				if (superseded) {
					if (!this.#debounce.has(questionId)) await this.#flush(questionId);
				} else {
					this.#pending.delete(questionId);
					this.#setStatus(questionId, 'saved');
				}
				return;
			case 'retry':
				this.#setStatus(questionId, 'unsaved');
				this.#scheduleRetry();
				return;
			case 'conflict':
				this.#pending.delete(questionId);
				void this.#o.store.remove(entry.attemptId, questionId, Number.MAX_SAFE_INTEGER).catch(() => undefined);
				this.#setStatus(questionId, 'error', result.message);
				this.#o.onConflict?.(result.message);
				return;
			case 'rejected':
				// Ошибка в данных: повтор не поможет, но новое изменение пользователя отправится.
				if (!superseded) this.#pending.delete(questionId);
				void this.#o.store.remove(entry.attemptId, questionId, entry.client_seq).catch(() => undefined);
				this.#setStatus(questionId, 'error', result.message);
				if (superseded) await this.#flush(questionId);
				return;
		}
	}

	#scheduleRetry(): void {
		if (this.#stopped || this.#retryTimer !== null) return;
		const delays = this.#o.retryDelays;
		const delay = delays[Math.min(this.#retryStep, delays.length - 1)] ?? 5000;
		this.#retryStep += 1;
		this.#retryTimer = this.#o.setTimer(() => {
			this.#retryTimer = null;
			for (const questionId of [...this.#pending.keys()]) {
				if (!this.#debounce.has(questionId)) void this.#flush(questionId);
			}
		}, delay);
	}

	#setStatus(questionId: string, status: SaveStatus, message?: string): void {
		this.#status.set(questionId, status);
		this.#o.onStatus(questionId, status, message);
	}
}

/** Хранилище в памяти — для тестов и как запасной вариант без IndexedDB. */
export class MemoryStore implements PendingStore {
	readonly entries = new Map<string, PendingAnswer>();

	#key(attemptId: string, questionId: string): string {
		return `${attemptId}:${questionId}`;
	}

	async load(attemptId: string): Promise<PendingAnswer[]> {
		return [...this.entries.values()].filter((entry) => entry.attemptId === attemptId);
	}

	async put(entry: PendingAnswer): Promise<void> {
		const key = this.#key(entry.attemptId, entry.questionId);
		const current = this.entries.get(key);
		if (!current || current.client_seq <= entry.client_seq) this.entries.set(key, { ...entry });
	}

	async remove(attemptId: string, questionId: string, clientSeq: number): Promise<void> {
		const key = this.#key(attemptId, questionId);
		const current = this.entries.get(key);
		if (current && current.client_seq <= clientSeq) this.entries.delete(key);
	}
}
