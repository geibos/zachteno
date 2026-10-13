import { expect, type BrowserContext, type Page } from '@playwright/test';
import { BASE_URL, SESSION_COOKIE, type Account } from './api';

/**
 * Открывает в браузере сессию, уже полученную через API (без лишних входов —
 * бэкенд ограничивает частоту входа). Если браузер не принял Secure-cookie
 * по http, входит через форму.
 */
export async function signIn(context: BrowserContext, page: Page, account: Account): Promise<void> {
	if (account.api.session) {
		// Chromium отклоняет Secure-cookie, если в addCookies передан http-адрес
		// («Invalid cookie fields»). С https-схемой та же host-only cookie сохраняется
		// и уходит на http://localhost (проверено отдельно). Порт для cookie не важен.
		const cookieUrl = new URL(BASE_URL);
		cookieUrl.protocol = 'https:';
		await context.addCookies([
			{
				name: SESSION_COOKIE,
				value: account.api.session,
				url: cookieUrl.origin,
				secure: true,
				httpOnly: true,
				sameSite: 'Lax'
			}
		]);
	}
	await page.goto('/');
	const signedIn = await page
		.getByRole('button', { name: 'Меню пользователя' })
		.waitFor({ timeout: 5_000 })
		.then(() => true)
		.catch(() => false);
	if (signedIn) return;
	await loginViaForm(page, account.email, account.password);
}

export async function loginViaForm(page: Page, email: string, password: string): Promise<void> {
	await page.goto('/login');
	await page.locator('#email').fill(email);
	await page.locator('#password').fill(password);
	await page.locator('form').getByRole('button', { name: 'Войти', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Меню пользователя' })).toBeVisible();
}

/** Тело `POST /api/signals` (сборщик сигналов антифрода). */
export interface SignalBody {
	event: string;
	marker: string;
	device: Record<string, string>;
	browser: Record<string, string>;
	typing: { dwell: number[]; flight: number[]; input: string } | null;
}

/** Ждёт отправку сигнала `event` страницей; возвращает тело запроса и статус ответа. */
export async function nextSignal(page: Page, event: string): Promise<{ body: SignalBody; status: number }> {
	const response = await page.waitForResponse(
		(candidate) =>
			candidate.url().endsWith('/api/signals') &&
			candidate.request().method() === 'POST' &&
			(candidate.request().postDataJSON() as SignalBody | null)?.event === event,
		{ timeout: 15_000 }
	);
	return { body: response.request().postDataJSON() as SignalBody, status: response.status() };
}

/** Остаток таймера в секундах из «мм:сс» или «ч:мм:сс». */
export function timerSeconds(text: string): number {
	const parts = text.trim().split(':').map(Number);
	return parts.reduce((total, part) => total * 60 + part, 0);
}

/** Идентификатор из URL вида /attempts/<id>. */
export function idFromUrl(page: Page, segment: string): string {
	const match = new RegExp(`/${segment}/([0-9a-f-]{36})`).exec(page.url());
	if (!match?.[1]) throw new Error(`В адресе ${page.url()} нет /${segment}/<id>`);
	return match[1];
}

/** Начать тест со страницы старта и дождаться раннера. Возвращает id попытки. */
export async function startAttempt(page: Page, testId: string): Promise<string> {
	await page.goto(`/tests/${testId}/start`);
	await page.getByRole('button', { name: 'Начать', exact: true }).click();
	await page.waitForURL(/\/attempts\/[0-9a-f-]{36}$/);
	await expect(page.getByText(/Вопрос 1 из \d+/)).toBeVisible();
	return idFromUrl(page, 'attempts');
}

/** Выбрать вариант по подписи и дождаться сохранения ответа. */
export async function choose(page: Page, option: string): Promise<void> {
	await page.getByLabel(option, { exact: true }).click();
	await expect(page.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
}
