/**
 * Ритм набора (design D14): только интервалы времени.
 *
 * - dwell — сколько клавиша удерживалась (keyup − keydown), мс;
 * - flight — пауза между отпусканием предыдущей и нажатием следующей, мс.
 *
 * Коды и символы клавиш НЕ сохраняются и не отправляются: идентификатор клавиши
 * живёт только в памяти между её keydown и keyup, чтобы связать пару событий.
 * Поля пароля не наблюдаются никогда — проверка и при подключении, и на каждом событии.
 */

export const MAX_SAMPLES = 256;

export interface TypingSnapshot {
	dwell: number[];
	flight: number[];
	input: 'touch' | 'keyboard';
}

/** Логика без DOM: принимает моменты нажатий и отпусканий. */
export class TypingRecorder {
	#dwell: number[] = [];
	#flight: number[] = [];
	/** Нажатые сейчас клавиши → время нажатия. Только для пары keydown/keyup, наружу не уходит. */
	readonly #down = new Map<string, number>();
	#lastUp: number | null = null;
	#touch = false;

	keyDown(pairId: string, at: number): void {
		if (this.#down.has(pairId)) return; // автоповтор при удержании
		if (this.#lastUp !== null) push(this.#flight, at - this.#lastUp);
		this.#down.set(pairId, at);
	}

	keyUp(pairId: string, at: number): void {
		const start = this.#down.get(pairId);
		this.#down.delete(pairId);
		if (start === undefined) return;
		push(this.#dwell, at - start);
		this.#lastUp = at;
	}

	markTouch(): void {
		this.#touch = true;
	}

	get size(): number {
		return this.#dwell.length + this.#flight.length;
	}

	/** Снимок для отправки; `null`, если данных нет. */
	snapshot(): TypingSnapshot | null {
		if (this.#dwell.length === 0 && this.#flight.length === 0) return null;
		return { dwell: [...this.#dwell], flight: [...this.#flight], input: this.#touch ? 'touch' : 'keyboard' };
	}

	reset(): void {
		this.#dwell = [];
		this.#flight = [];
		this.#down.clear();
		this.#lastUp = null;
	}
}

function push(list: number[], value: number): void {
	if (!Number.isFinite(value)) return;
	list.push(Math.round(Math.max(-5000, Math.min(value, 5000))));
	if (list.length > MAX_SAMPLES) list.splice(0, list.length - MAX_SAMPLES);
}

interface FieldLike {
	type?: unknown;
	autocomplete?: unknown;
	getAttribute?: (name: string) => string | null;
}

/** Поле пароля (или похожее на него) — наблюдать запрещено. */
export function isPasswordField(target: unknown): boolean {
	if (typeof target !== 'object' || target === null) return false;
	const field = target as FieldLike;
	const type = String(field.type ?? field.getAttribute?.('type') ?? '').toLowerCase();
	const autocomplete = String(field.autocomplete ?? field.getAttribute?.('autocomplete') ?? '').toLowerCase();
	return type === 'password' || autocomplete.includes('password');
}

interface KeyLikeEvent extends Event {
	readonly code?: string;
	readonly key?: string;
	readonly pointerType?: string;
}

/**
 * Подписывает `recorder` на события набора внутри `root` (делегирование).
 * `accept(target)` выбирает поля (например, email). Возвращает функцию отписки.
 */
export function observeTyping(
	root: EventTarget,
	recorder: TypingRecorder,
	accept: (target: EventTarget) => boolean,
	now: () => number = () => performance.now()
): () => void {
	const allowed = (event: Event): boolean => {
		const target = event.target;
		return target !== null && !isPasswordField(target) && accept(target);
	};
	// Идентификатор пары — физическая клавиша; используется только как ключ Map в памяти.
	const pairId = (event: KeyLikeEvent): string => event.code || event.key || 'unknown';

	const onKeyDown = (event: Event) => {
		if (allowed(event)) recorder.keyDown(pairId(event as KeyLikeEvent), now());
	};
	const onKeyUp = (event: Event) => {
		if (allowed(event)) recorder.keyUp(pairId(event as KeyLikeEvent), now());
	};
	const onPointer = (event: Event) => {
		if (allowed(event) && (event as KeyLikeEvent).pointerType === 'touch') recorder.markTouch();
	};
	const onTouch = (event: Event) => {
		if (allowed(event)) recorder.markTouch();
	};

	root.addEventListener('keydown', onKeyDown, true);
	root.addEventListener('keyup', onKeyUp, true);
	root.addEventListener('pointerdown', onPointer, true);
	root.addEventListener('touchstart', onTouch, true);
	return () => {
		root.removeEventListener('keydown', onKeyDown, true);
		root.removeEventListener('keyup', onKeyUp, true);
		root.removeEventListener('pointerdown', onPointer, true);
		root.removeEventListener('touchstart', onTouch, true);
	};
}

/** Svelte-action: `use:typingRhythm={{ recorder, accept }}` на форме или контейнере. */
export function typingRhythm(
	node: HTMLElement,
	params: { recorder: TypingRecorder; accept: (target: EventTarget) => boolean }
): { destroy: () => void } {
	const stop = observeTyping(node, params.recorder, params.accept);
	return { destroy: stop };
}

/** Поле email (по типу или id). */
export function isEmailField(target: EventTarget): boolean {
	const field = target as FieldLike & { id?: string };
	return String(field.type ?? '').toLowerCase() === 'email' || field.id === 'email';
}
