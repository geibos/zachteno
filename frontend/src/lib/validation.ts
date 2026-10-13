/** Клиентские проверки форм; правила совпадают с бэкендом (spec identity-auth). */

export const PASSWORD_MIN = 12;
export const PASSWORD_MAX = 128;
export const NAME_MAX = 200;
export const EMAIL_MAX = 254;

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** Длина в символах (кодовых точках), как `chars().count()` в Rust. */
function charCount(value: string): number {
	return [...value].length;
}

export function validateEmail(value: string): string | null {
	const email = value.trim();
	if (email === '') return 'Укажите email.';
	if (email.length > EMAIL_MAX || !EMAIL_RE.test(email)) return 'Похоже, в email опечатка.';
	return null;
}

export function validateName(value: string): string | null {
	const length = charCount(value.trim());
	if (length === 0) return 'Укажите имя.';
	if (length > NAME_MAX) return `Имя не длиннее ${NAME_MAX} символов.`;
	return null;
}

export function validateNewPassword(value: string): string | null {
	const length = charCount(value);
	if (length === 0) return 'Придумайте пароль.';
	if (length < PASSWORD_MIN) return `Пароль должен быть не короче ${PASSWORD_MIN} символов.`;
	if (length > PASSWORD_MAX) return `Пароль должен быть не длиннее ${PASSWORD_MAX} символов.`;
	return null;
}

export function validateRepeat(password: string, repeat: string): string | null {
	if (repeat === '') return 'Повторите пароль.';
	return password === repeat ? null : 'Пароли не совпадают.';
}

/** Безопасный адрес возврата после входа: только локальный путь. */
export function safeNext(raw: string | null | undefined, fallback = '/'): string {
	if (!raw) return fallback;
	if (!raw.startsWith('/') || raw.startsWith('//') || raw.startsWith('/\\')) return fallback;
	// Управляющие символы и обратные слэши браузеры местами трактуют как «//».
	// eslint-disable-next-line no-control-regex
	if (/[\u0000-\u001f\\]/.test(raw)) return fallback;
	return raw;
}
