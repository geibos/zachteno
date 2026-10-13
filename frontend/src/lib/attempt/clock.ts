/**
 * Таймер попытки. Срок хранится на сервере (`deadline_at`), клиент только
 * показывает остаток. Часы устройства могут врать, поэтому считаем поправку
 * по `server_now` из ответа сервера.
 */

/** Поправка (мс), которую надо прибавить к часам клиента, чтобы получить серверное время. */
export function clockOffset(serverNowIso: string, clientNowMs: number): number {
	const server = Date.parse(serverNowIso);
	if (Number.isNaN(server)) return 0;
	return server - clientNowMs;
}

/** Сколько осталось до срока (мс, не меньше нуля); `null` — лимита нет. */
export function remainingMs(
	deadlineIso: string | null | undefined,
	offsetMs: number,
	clientNowMs: number
): number | null {
	if (!deadlineIso) return null;
	const deadline = Date.parse(deadlineIso);
	if (Number.isNaN(deadline)) return null;
	return Math.max(0, deadline - (clientNowMs + offsetMs));
}

/** Порог «мало времени» для подсветки таймера. */
export const LOW_TIME_MS = 60_000;
