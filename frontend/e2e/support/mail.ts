/**
 * Письма стенда в режиме `MAIL_MODE=directory`: бэкенд пишет каждое письмо файлом
 * `<uuid>.eml` в `MAIL_DIR`. Тест читает этот каталог (`E2E_MAIL_DIR` — тот же путь,
 * что `MAIL_DIR` сервера) и достаёт ссылку из письма нужному адресату.
 */
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';

export const MAIL_DIR = process.env.E2E_MAIL_DIR ?? '';
export const NO_MAIL_REASON = 'Нужен E2E_MAIL_DIR — каталог писем стенда (MAIL_MODE=directory, тот же путь, что MAIL_DIR).';

interface Letter {
	headers: string;
	body: string;
}

/** Заголовки и декодированный текст письма (7bit, quoted-printable или base64). */
export function parseLetter(raw: string): Letter {
	const normalized = raw.replace(/\r\n/g, '\n');
	const split = normalized.indexOf('\n\n');
	const headers = split === -1 ? normalized : normalized.slice(0, split);
	const encoded = split === -1 ? '' : normalized.slice(split + 2);
	const encoding = /^content-transfer-encoding:\s*(\S+)/im.exec(headers)?.[1]?.toLowerCase() ?? '7bit';
	let body = encoded;
	if (encoding === 'base64') {
		body = Buffer.from(encoded.replace(/\s+/g, ''), 'base64').toString('utf8');
	} else if (encoding === 'quoted-printable') {
		const bytes = encoded
			.replace(/=\n/g, '')
			.replace(/=([0-9A-F]{2})/gi, (_, hex: string) => String.fromCharCode(parseInt(hex, 16)));
		body = Buffer.from(bytes, 'latin1').toString('utf8');
	}
	return { headers, body };
}

/** Заголовок `To` содержит адрес (без учёта регистра). */
function addressedTo(headers: string, email: string): boolean {
	const to = /^to:\s*(.+)$/im.exec(headers)?.[1] ?? '';
	return to.toLowerCase().includes(email.toLowerCase());
}

/**
 * Ждёт письмо адресату со ссылкой на `path` (например, `/verify-email`) и возвращает
 * путь с query-строкой — без хоста, чтобы открыть его на `baseURL` теста.
 * `skip` — письма, уже прочитанные раньше (например, прежняя ссылка того же вида).
 */
export async function waitForLink(email: string, path: string, skip: string[] = []): Promise<string> {
	const pattern = new RegExp(`https?://[^\\s/]+(${path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\?token=[A-Za-z0-9_-]+)`);
	let found: string | null = null;
	await expect
		.poll(
			async () => {
				const files = (await readdir(MAIL_DIR).catch(() => [] as string[])).filter((name) => name.endsWith('.eml'));
				for (const name of files) {
					const letter = parseLetter(await readFile(join(MAIL_DIR, name), 'utf8'));
					if (!addressedTo(letter.headers, email)) continue;
					const link = pattern.exec(letter.body)?.[1];
					if (link && !skip.includes(link)) {
						found = link;
						return true;
					}
				}
				return false;
			},
			{ message: `письмо со ссылкой ${path} для ${email}`, timeout: 20_000 }
		)
		.toBe(true);
	return found!;
}
