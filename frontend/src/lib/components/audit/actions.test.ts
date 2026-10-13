import { describe, expect, it } from 'vitest';
import { actionLabel, detailsText, objectTypeLabel } from './actions';

describe('аудит: подписи', () => {
	it('известные действия по-русски, неизвестные — кодом', () => {
		expect(actionLabel('auth.login')).toBe('Вход');
		expect(actionLabel('auth.login_failed')).toBe('Неудачный вход');
		expect(actionLabel('something.new')).toBe('something.new');
		expect(objectTypeLabel('course')).toBe('курс');
		expect(objectTypeLabel('x')).toBe('x');
	});

	it('details: пустой объект скрывается', () => {
		expect(detailsText({})).toBeNull();
		expect(detailsText(null)).toBeNull();
		expect(detailsText({ role: 'methodist' })).toContain('"role": "methodist"');
	});
});
