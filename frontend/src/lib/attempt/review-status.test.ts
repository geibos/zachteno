import { describe, expect, it } from 'vitest';
import { reviewStatusText } from './review-status';

describe('reviewStatusText', () => {
	it('доступно / никогда', () => {
		expect(reviewStatusText({ available: true, mode: 'after_submit', waiting_for_stream: false })).toBe(
			'Результаты доступны'
		);
		expect(reviewStatusText({ available: false, mode: 'never', waiting_for_stream: false })).toMatch(
			/не показываются/
		);
	});

	it('после закрытия: окно открыто или ждём чужие попытки', () => {
		const review = { available: false, mode: 'after_close' as const, waiting_for_stream: true };
		expect(reviewStatusText(review, true)).toBe('Результаты будут доступны после закрытия теста');
		expect(reviewStatusText(review, false)).toBe(
			'Результаты будут доступны после завершения всех попыток потока'
		);
	});

	it('после закрытия с датой — по МСК', () => {
		const text = reviewStatusText({
			available: false,
			mode: 'after_close',
			waiting_for_stream: false,
			not_before: '2026-12-01T09:00:00Z'
		});
		expect(text).toMatch(/не раньше 1 декабря 2026 г\.?,? (в )?12:00 МСК/);
	});
});
