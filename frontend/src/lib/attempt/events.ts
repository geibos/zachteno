import type { Schema } from '$lib/api/client';

export type ClientEventKind = Schema<'ClientEventKind'>;

/**
 * Копит события вкладки (`visibilitychange`, `online`/`offline`) и отправляет пачками.
 * Неотправленное остаётся в буфере до следующей попытки.
 */
export class EventBatcher {
	#buffer: ClientEventKind[] = [];
	#timer: ReturnType<typeof setTimeout> | null = null;
	#sending = false;
	readonly #send: (events: ClientEventKind[], keepalive: boolean) => Promise<void>;
	readonly #delayMs: number;

	constructor(send: (events: ClientEventKind[], keepalive: boolean) => Promise<void>, delayMs = 3000) {
		this.#send = send;
		this.#delayMs = delayMs;
	}

	push(kind: ClientEventKind): void {
		this.#buffer.push(kind);
		if (this.#buffer.length > 200) this.#buffer.splice(0, this.#buffer.length - 200);
		if (this.#timer === null) this.#timer = setTimeout(() => void this.flush(), this.#delayMs);
	}

	async flush(keepalive = false): Promise<void> {
		if (this.#timer !== null) {
			clearTimeout(this.#timer);
			this.#timer = null;
		}
		if (this.#sending || this.#buffer.length === 0) return;
		if (typeof navigator !== 'undefined' && !navigator.onLine && !keepalive) {
			this.#timer = setTimeout(() => void this.flush(), this.#delayMs * 5);
			return;
		}
		const batch = this.#buffer;
		this.#buffer = [];
		this.#sending = true;
		try {
			await this.#send(batch, keepalive);
		} catch {
			this.#buffer = [...batch, ...this.#buffer];
			if (this.#timer === null) this.#timer = setTimeout(() => void this.flush(), this.#delayMs * 5);
		} finally {
			this.#sending = false;
		}
	}

	dispose(): void {
		if (this.#timer !== null) clearTimeout(this.#timer);
		this.#timer = null;
	}
}
