/** Форматирование для интерфейса. Время пользователям показываем по МСК (D3). */

const dateTime = new Intl.DateTimeFormat('ru-RU', {
	timeZone: 'Europe/Moscow',
	day: 'numeric',
	month: 'long',
	year: 'numeric',
	hour: '2-digit',
	minute: '2-digit'
});

export function formatDateTime(iso: string): string {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return iso;
	return `${dateTime.format(date)} МСК`;
}

/** Короткое описание браузера и ОС по User-Agent — для списка сессий. */
export function describeUserAgent(ua: string | null | undefined): string {
	if (!ua) return 'Неизвестное устройство';
	const browser =
		/YaBrowser\//.test(ua) ? 'Яндекс Браузер'
		: /Edg\//.test(ua) ? 'Edge'
		: /OPR\//.test(ua) ? 'Opera'
		: /Firefox\//.test(ua) ? 'Firefox'
		: /Chrome\//.test(ua) ? 'Chrome'
		: /Safari\//.test(ua) ? 'Safari'
		: null;
	const os =
		/iPhone|iPad|iPod/.test(ua) ? 'iOS'
		: /Android/.test(ua) ? 'Android'
		: /Windows/.test(ua) ? 'Windows'
		: /Mac OS X|Macintosh/.test(ua) ? 'macOS'
		: /Linux/.test(ua) ? 'Linux'
		: null;
	if (browser && os) return `${browser}, ${os}`;
	return browser ?? os ?? 'Неизвестное устройство';
}

/** Инициалы для аватара. */
export function initials(name: string): string {
	const parts = name.trim().split(/\s+/).filter(Boolean);
	const letters = parts.slice(0, 2).map((part) => [...part][0] ?? '');
	return letters.join('').toUpperCase() || '?';
}
