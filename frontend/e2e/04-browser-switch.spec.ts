import { existsSync } from 'node:fs';
import { chromium, firefox, type BrowserType } from '@playwright/test';
import { test, expect } from './support/fixtures';
import { BASE_URL, HAS_ADMIN, NO_ADMIN_REASON, registerUser } from './support/api';
import { loginViaForm } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

function firefoxInstalled(): boolean {
	try {
		return existsSync(firefox.executablePath());
	} catch {
		return false;
	}
}

interface SignalBody {
	event: string;
	device: Record<string, string>;
	browser: Record<string, string>;
}

/** Входит через форму и перехватывает тело POST /api/signals (событие login). */
async function captureLoginSignal(type: BrowserType, email: string, password: string): Promise<SignalBody> {
	const browser = await type.launch();
	try {
		const context = await browser.newContext({ baseURL: BASE_URL, locale: 'ru-RU', timezoneId: 'Europe/Moscow' });
		const page = await context.newPage();
		const captured = new Promise<SignalBody>((resolve) => {
			void page.route('**/api/signals', async (route) => {
				const body = route.request().postDataJSON() as SignalBody;
				if (body.event === 'login') resolve(body);
				await route.continue();
			});
		});
		await loginViaForm(page, email, password);
		return await Promise.race([
			captured,
			new Promise<never>((_, reject) => setTimeout(() => reject(new Error('сигнал login не отправлен')), 15_000))
		]);
	} finally {
		await browser.close();
	}
}

test('9. смена браузера на одной машине: сравнение отпечатка устройства Chromium ↔ Firefox', async ({}, testInfo) => {
	test.skip(
		!firefoxInstalled(),
		'Firefox для Playwright не установлен: выполните `npx playwright install firefox`, чтобы сравнить отпечатки.'
	);
	const user = await registerUser('switch', 'Студент Два Браузера');
	await user.api.dispose();

	const fromChromium = await captureLoginSignal(chromium, user.email, user.password);
	const fromFirefox = await captureLoginSignal(firefox, user.email, user.password);

	const keys = [...new Set([...Object.keys(fromChromium.device), ...Object.keys(fromFirefox.device)])].sort();
	const matched = keys.filter((key) => fromChromium.device[key] !== undefined && fromChromium.device[key] === fromFirefox.device[key]);
	const differed = keys.filter((key) => !matched.includes(key));
	const report = [
		`Совпали (${matched.length}): ${matched.join(', ') || '—'}`,
		`Различаются или есть только в одном браузере (${differed.length}): ${differed
			.map((key) => `${key} [chromium=${fromChromium.device[key] ?? '∅'}; firefox=${fromFirefox.device[key] ?? '∅'}]`)
			.join('; ') || '—'}`
	].join('\n');
	console.log(report);
	testInfo.annotations.push({ type: 'device-match', description: report });
	await testInfo.attach('device-comparison.txt', { body: report, contentType: 'text/plain' });

	expect(Object.keys(fromChromium.device).length).toBeGreaterThan(0);
	expect(Object.keys(fromFirefox.device).length).toBeGreaterThan(0);
	// Сколько признаков совпадёт, зависит от машины — тест только фиксирует отчёт.
});
