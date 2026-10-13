import { describe, expect, it } from 'vitest';
import { safeNext, validateEmail, validateName, validateNewPassword, validateRepeat } from './validation';

describe('safeNext', () => {
	it('пропускает локальные пути', () => {
		expect(safeNext('/profile')).toBe('/profile');
		expect(safeNext('/courses?tab=1#x')).toBe('/courses?tab=1#x');
	});

	it('отбрасывает внешние и протокольно-относительные адреса', () => {
		for (const bad of ['https://evil.example', '//evil.example', '/\\evil.example', 'javascript:alert(1)', '/\tx']) {
			expect(safeNext(bad)).toBe('/');
		}
		expect(safeNext(null, '/x')).toBe('/x');
	});
});

describe('валидация', () => {
	it('email', () => {
		expect(validateEmail('')).not.toBeNull();
		expect(validateEmail('a@b')).not.toBeNull();
		expect(validateEmail(' student@example.org ')).toBeNull();
	});

	it('имя', () => {
		expect(validateName('   ')).not.toBeNull();
		expect(validateName('Анна')).toBeNull();
		expect(validateName('я'.repeat(201))).not.toBeNull();
	});

	it('пароль: только длина, без требований к составу', () => {
		expect(validateNewPassword('короткий')).not.toBeNull();
		expect(validateNewPassword('длинныйпарольбезцифр')).toBeNull();
		expect(validateNewPassword('x'.repeat(129))).not.toBeNull();
		// 12 эмодзи — 12 символов, хотя в UTF-16 их 24
		expect(validateNewPassword('😀'.repeat(12))).toBeNull();
	});

	it('повтор пароля', () => {
		expect(validateRepeat('abc', 'abd')).not.toBeNull();
		expect(validateRepeat('abc', 'abc')).toBeNull();
	});
});
