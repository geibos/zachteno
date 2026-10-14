/**
 * Список адресов из вставленного текста: столбец из Excel, строка через запятую
 * или точку с запятой. Повторы (без учёта регистра) убираются, порядок сохраняется.
 * Проверку формата делает сервер — он же сообщит «некорректный адрес».
 */
export function parseEmailList(text: string): string[] {
	const seen = new Set<string>();
	const emails: string[] = [];
	for (const token of text.split(/[\s,;]+/)) {
		const email = token.trim();
		const key = email.toLowerCase();
		if (email === '' || seen.has(key)) continue;
		seen.add(key);
		emails.push(email);
	}
	return emails;
}
