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

/** МСК — UTC+3 без перехода на летнее время. */
const MSK_OFFSET_MS = 3 * 60 * 60 * 1000;

function pad(value: number): string {
	return String(value).padStart(2, '0');
}

/** ISO (UTC) → значение для `<input type="datetime-local">` по МСК. */
export function isoToMskInput(iso: string | null | undefined): string {
	if (!iso) return '';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '';
	const msk = new Date(date.getTime() + MSK_OFFSET_MS);
	return (
		`${msk.getUTCFullYear()}-${pad(msk.getUTCMonth() + 1)}-${pad(msk.getUTCDate())}` +
		`T${pad(msk.getUTCHours())}:${pad(msk.getUTCMinutes())}`
	);
}

/** Значение `datetime-local`, введённое по МСК → ISO в UTC; пустое → `null`. */
export function mskInputToIso(value: string): string | null {
	const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value.trim());
	if (!match) return null;
	const [, y, mo, d, h, mi, s] = match;
	const utc = Date.UTC(Number(y), Number(mo) - 1, Number(d), Number(h), Number(mi), Number(s ?? 0));
	return new Date(utc - MSK_OFFSET_MS).toISOString();
}

/** Оставшееся время: «1:05:09» или «05:09». */
export function formatDuration(ms: number): string {
	const total = Math.max(0, Math.ceil(ms / 1000));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const seconds = total % 60;
	return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${pad(minutes)}:${pad(seconds)}`;
}

/** Десятичное из API («1.50») → «1,5». */
export function formatScore(value: string | null | undefined): string {
	if (value === null || value === undefined || value === '') return '—';
	const number = Number(value);
	if (Number.isNaN(number)) return value;
	return number.toLocaleString('ru-RU', { maximumFractionDigits: 2 });
}

/**
 * Ввод десятичного числа (запятая или точка) → строка для API или `null`, если ввод некорректен.
 * Пустая строка → `''`.
 */
export function normalizeDecimal(input: string): string | null {
	const value = input.trim().replace(',', '.').replace('−', '-');
	if (value === '') return '';
	if (!/^-?\d+(\.\d{1,2})?$/.test(value)) return null;
	return value;
}

/** Процент с одним знаком: «66,7 %»; пусто — когда процента нет. */
export function formatPercent(value: string | null | undefined): string {
	if (value === null || value === undefined || value === '') return '';
	const number = Number(value);
	if (Number.isNaN(number)) return '';
	return `${number.toLocaleString('ru-RU', { maximumFractionDigits: 1 })} %`;
}

/** Склонение: plural(5, ['вопрос', 'вопроса', 'вопросов']). */
export function plural(n: number, forms: [string, string, string]): string {
	const mod10 = Math.abs(n) % 10;
	const mod100 = Math.abs(n) % 100;
	if (mod10 === 1 && mod100 !== 11) return forms[0];
	if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return forms[1];
	return forms[2];
}

/** «5 вопросов». */
export function count(n: number, forms: [string, string, string]): string {
	return `${n} ${plural(n, forms)}`;
}
