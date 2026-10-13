import { defineConfig, devices } from '@playwright/test';

/**
 * Сквозные тесты против живого стенда (задача 14.2).
 * Стенд: бэкенд с собранной статикой, по умолчанию http://localhost:8080.
 * Данные создаются только через публичный API этого же стенда; суперпользователь —
 * из E2E_ADMIN_EMAIL / E2E_ADMIN_PASSWORD. Письма (подтверждение email, восстановление
 * пароля) тесты читают из E2E_MAIL_DIR — каталога MAIL_DIR стенда с MAIL_MODE=directory.
 * Запуск: `npm run e2e`.
 */
const baseURL = process.env.E2E_BASE_URL ?? 'http://localhost:8080';

export default defineConfig({
	testDir: './e2e',
	// Один стенд и один суперпользователь: последовательно, чтобы не упереться в лимиты входа.
	workers: 1,
	fullyParallel: false,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	timeout: 90_000,
	expect: { timeout: 10_000 },
	// HTML-отчёт сохраняется всегда (задача 14.2): frontend/playwright-report/, в git не попадает.
	reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],
	use: {
		baseURL,
		locale: 'ru-RU',
		timezoneId: 'Europe/Moscow',
		trace: 'retain-on-failure',
		screenshot: 'only-on-failure'
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }]
});
