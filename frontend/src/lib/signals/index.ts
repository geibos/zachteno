/**
 * Отправка сигналов антифрода (design D14): `POST /api/signals`.
 * Ошибки молча игнорируются — сбор сигналов не должен мешать пользователю.
 */
import type { Schema } from '$lib/api/client';
import { browserMarkerStores, ensureMarker } from './marker';
import { collectBrowser, collectDevice, type Components } from './fingerprint';
import type { TypingSnapshot } from './typing';

export type SignalEvent = Schema<'SignalEvent'>;

export { TypingRecorder, typingRhythm, isEmailField } from './typing';

const ANSWER_INTERVAL_MS = 60_000;

let cache: Promise<{ marker: string; browser: Components; device: Components }> | null = null;
let lastAnswerAt = 0;

/** Замеры дорогие (canvas, audio, шрифты) — делаем один раз за загрузку страницы. */
function collect(): Promise<{ marker: string; browser: Components; device: Components }> {
	cache ??= Promise.all([ensureMarker(browserMarkerStores), collectBrowser(), collectDevice()]).then(
		([marker, browser, device]) => ({ marker, browser, device })
	);
	return cache;
}

/**
 * Отправляет сигнал. Для `answer` — не чаще раза в минуту (вызовы чаще молча пропускаются;
 * возвращает `false`, чтобы вызывающий не сбрасывал накопленный ритм набора).
 */
export function sendSignals(event: SignalEvent, typing?: TypingSnapshot | null): boolean {
	if (typeof window === 'undefined') return false;
	if (event === 'answer') {
		const now = Date.now();
		if (now - lastAnswerAt < ANSWER_INTERVAL_MS) return false;
		lastAnswerAt = now;
	}
	void (async () => {
		try {
			const { marker, browser, device } = await collect();
			const body: Schema<'SignalInput'> = {
				event,
				marker,
				browser,
				device,
				timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? null,
				typing: typing ?? null
			};
			await fetch('/api/signals', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(body),
				credentials: 'same-origin',
				keepalive: true
			});
		} catch {
			// сигнал не критичен
		}
	})();
	return true;
}
