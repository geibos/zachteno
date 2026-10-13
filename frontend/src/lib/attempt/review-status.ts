import type { Schema } from '$lib/api/client';
import { formatDateTime } from '$lib/format';

type Availability = Schema<'ReviewAvailability'>;

/**
 * Текст о доступности результатов (spec test-taking, «Просмотр результатов»).
 * `isOpen` — открыто ли окно теста для потока студента (если известно).
 */
export function reviewStatusText(review: Availability | null | undefined, isOpen?: boolean): string {
	if (!review) return 'Результаты пока недоступны';
	if (review.available) return 'Результаты доступны';
	if (review.mode === 'never') return 'Результаты по этому тесту не показываются';
	if (review.mode === 'after_submit') return 'Результаты появятся после сдачи';

	const parts: string[] = [];
	if (review.waiting_for_stream) {
		if (isOpen === true) parts.push('после закрытия теста');
		else if (isOpen === false) parts.push('после завершения всех попыток потока');
		else parts.push('после закрытия теста и завершения всех попыток потока');
	}
	if (review.not_before) parts.push(`не раньше ${formatDateTime(review.not_before)}`);
	if (parts.length === 0) parts.push('после закрытия теста');
	return `Результаты будут доступны ${parts.join(', ')}`;
}

/** Причина сдачи для подписи. */
export function submitReasonText(reason: string | null | undefined): string | null {
	switch (reason) {
		case 'timeout':
			return 'сдана по времени';
		case 'window_closed':
			return 'сдана при закрытии теста';
		default:
			return null;
	}
}
