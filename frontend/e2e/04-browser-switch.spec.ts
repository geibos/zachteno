import { existsSync } from 'node:fs';
import { chromium, firefox, type BrowserType, type Page } from '@playwright/test';
import { test, expect } from './support/fixtures';
import { BASE_URL, HAS_ADMIN, NO_ADMIN_REASON, enrolledStudent, newPassword, registerUser, seedTest, unique, uniqueEmail } from './support/api';
import { loginViaForm, nextSignal, type SignalBody } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

function firefoxInstalled(): boolean {
	try {
		return existsSync(firefox.executablePath());
	} catch {
		return false;
	}
}

const FIREFOX_MISSING =
	'Firefox для Playwright не установлен: выполните `npx playwright install firefox`, чтобы сравнить отпечатки.';

interface Captured {
	body: SignalBody;
	status: number;
}

async function withPage<T>(type: BrowserType, action: (page: Page) => Promise<T>): Promise<T> {
	const browser = await type.launch();
	try {
		const context = await browser.newContext({ baseURL: BASE_URL, locale: 'ru-RU', timezoneId: 'Europe/Moscow' });
		return await action(await context.newPage());
	} finally {
		await browser.close();
	}
}

/** Входит через форму и возвращает тело POST /api/signals (событие login) и статус ответа. */
async function captureLoginSignal(type: BrowserType, email: string, password: string): Promise<Captured> {
	return withPage(type, async (page) => {
		const signal = nextSignal(page, 'login');
		await loginViaForm(page, email, password);
		return signal;
	});
}

function compareDevices(left: SignalBody, right: SignalBody): string {
	const keys = [...new Set([...Object.keys(left.device), ...Object.keys(right.device)])].sort();
	const matched = keys.filter((key) => left.device[key] !== undefined && left.device[key] === right.device[key]);
	const differed = keys.filter((key) => !matched.includes(key));
	return [
		`Совпали (${matched.length}): ${matched.join(', ') || '—'}`,
		`Различаются или есть только в одном браузере (${differed.length}): ${differed
			.map((key) => `${key} [chromium=${left.device[key] ?? '∅'}; firefox=${right.device[key] ?? '∅'}]`)
			.join('; ') || '—'}`
	].join('\n');
}

test('9. смена браузера на одной машине: сравнение отпечатка устройства Chromium ↔ Firefox', async ({}, testInfo) => {
	test.skip(!firefoxInstalled(), FIREFOX_MISSING);
	const user = await registerUser('switch', 'Студент Два Браузера');
	await user.api.dispose();

	const fromChromium = await captureLoginSignal(chromium, user.email, user.password);
	const fromFirefox = await captureLoginSignal(firefox, user.email, user.password);

	// Сигнал входа из каждого браузера принят и сохранён (204).
	expect(fromChromium.status).toBe(204);
	expect(fromFirefox.status).toBe(204);
	// Маркеры у браузеров свои.
	expect(fromChromium.body.marker).not.toBe(fromFirefox.body.marker);

	const report = compareDevices(fromChromium.body, fromFirefox.body);
	console.log(report);
	testInfo.annotations.push({ type: 'device-match', description: report });
	await testInfo.attach('device-comparison.txt', { body: report, contentType: 'text/plain' });

	expect(Object.keys(fromChromium.body.device).length).toBeGreaterThan(0);
	expect(Object.keys(fromFirefox.body.device).length).toBeGreaterThan(0);
	// Сколько признаков совпадёт, зависит от машины — тест только фиксирует отчёт.
});

interface CourseLinks {
	links: Array<{ user_a: { id: string }; user_b: { id: string }; evidence: Array<{ kind: string; detail: string }> }>;
}

test('10. смена браузера: второй аккаунт из Firefox связан с первым по отпечатку устройства', async ({ admin }, testInfo) => {
	test.skip(!firefoxInstalled(), FIREFOX_MISSING);
	const seeded = await seedTest(admin.api);
	const first = await enrolledStudent(seeded, unique('Студент Хром'));
	const fromChromium = await captureLoginSignal(chromium, first.email, first.password);
	expect(fromChromium.status).toBe(204);

	// Второй аккаунт регистрируется по приглашению в другом браузере на том же компьютере.
	const secondEmail = uniqueEmail('switch-second');
	const fromFirefox = await withPage(firefox, async (page) => {
		const signal = nextSignal(page, 'register');
		await page.goto(`/invite/${seeded.inviteToken}`);
		await page.locator('#email').fill(secondEmail);
		await page.locator('#name').fill(unique('Студент Лис'));
		const password = newPassword();
		await page.locator('#password').fill(password);
		await page.locator('#repeat').fill(password);
		await page.getByRole('button', { name: 'Зарегистрироваться и вступить' }).click();
		await page.waitForURL((url) => url.pathname === '/');
		return signal;
	});
	expect(fromFirefox.status).toBe(204);
	expect(fromFirefox.body.marker).not.toBe(fromChromium.body.marker);
	const report = compareDevices(fromChromium.body, fromFirefox.body);
	await testInfo.attach('device-comparison.txt', { body: report, contentType: 'text/plain' });
	const stream = await admin.api.call<{ members: Array<{ user_id: string; email: string }> }>(
		'GET',
		`/api/streams/${seeded.streamId}`
	);
	const secondId = stream.members.find((member) => member.email === secondEmail)?.user_id;
	expect(secondId, 'второй аккаунт вступил в поток').toBeTruthy();
	const pair = new Set([first.user.id, secondId]);

	// Спека «Смена браузера на том же устройстве»: отпечаток устройства совпадает,
	// поэтому связь аккаунтов есть и в её основаниях — общее устройство.
	await expect
		.poll(
			async () => {
				const { links } = await admin.api.call<CourseLinks>('GET', `/api/courses/${seeded.courseId}/antifraud`);
				const link = links.find((item) => pair.has(item.user_a.id) && pair.has(item.user_b.id));
				return (link?.evidence.map((evidence) => evidence.kind) ?? []).sort();
			},
			{ message: `основания связи Chromium ↔ Firefox должны включать общее устройство\n${report}`, timeout: 30_000 }
		)
		.toContainEqual(expect.stringMatching(/^shared_(rare|common)_device$/));
	await first.api.dispose();
});
