import { test, expect } from './support/fixtures';
import { Api, HAS_ADMIN, NO_ADMIN_REASON, SESSION_COOKIE, enrolledStudent, seedTest, unique, uniqueEmail } from './support/api';
import { loginViaForm, nextSignal } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

const MARKER_COOKIE = 'zt_m';

interface CourseLinks {
	links: Array<{ user_a: { id: string }; user_b: { id: string }; evidence: Array<{ kind: string }> }>;
}

test('20. частичная очистка: без cookie zt_m маркер восстанавливается из других хранилищ, связь сохраняется', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api);
	const first = await enrolledStudent(seeded, unique('Студент Маркер Один'));
	const second = await enrolledStudent(seeded, unique('Студент Маркер Два'));
	const markerCookie = async () => (await context.cookies()).find((cookie) => cookie.name === MARKER_COOKIE)?.value;

	// Первый аккаунт входит — браузер получает маркер во всех трёх хранилищах.
	const firstSignal = nextSignal(page, 'login');
	await loginViaForm(page, first.email, first.password);
	const { body: firstBody, status: firstStatus } = await firstSignal;
	expect(firstStatus).toBe(204);
	const marker = firstBody.marker;
	expect(marker).toMatch(/^[0-9a-f]{32}$/);
	await expect.poll(markerCookie).toBe(marker);
	expect(await page.evaluate((key) => localStorage.getItem(key), MARKER_COOKIE)).toBe(marker);

	// Удаляем только cookie (маркер и сессию); localStorage и IndexedDB остаются.
	await context.clearCookies({ name: MARKER_COOKIE });
	await context.clearCookies({ name: SESSION_COOKIE });
	expect(await markerCookie()).toBeUndefined();

	// Второй аккаунт в том же браузере: маркер прежний, cookie восстановлена.
	const secondSignal = nextSignal(page, 'login');
	await loginViaForm(page, second.email, second.password);
	const { body: secondBody, status: secondStatus } = await secondSignal;
	expect(secondStatus).toBe(204);
	expect(secondBody.marker).toBe(marker);
	await expect.poll(markerCookie).toBe(marker);

	// Связь аккаунтов по общему маркеру видна проверяющему курса.
	const pair = new Set([first.user.id, second.user.id]);
	await expect
		.poll(
			async () => {
				const { links } = await admin.api.call<CourseLinks>('GET', `/api/courses/${seeded.courseId}/antifraud`);
				const link = links.find((item) => pair.has(item.user_a.id) && pair.has(item.user_b.id));
				return link?.evidence.map((evidence) => evidence.kind) ?? [];
			},
			{ message: 'связь двух аккаунтов по маркеру', timeout: 30_000 }
		)
		.toContain('shared_marker');
	await first.api.dispose();
	await second.api.dispose();
});

test('21. ввод в поле пароля не порождает событий динамики набора', async ({ page }) => {
	// Пароль латиницей: так каждый символ — настоящие keydown/keyup (кириллицу Playwright вставляет без клавиш).
	const email = uniqueEmail('typing');
	const password = `latin typing check ${Math.random().toString(36).slice(2, 10)}`;
	const api = await Api.create();
	await api.call('POST', '/api/auth/register', { email, display_name: 'Студент Набор', password });
	await api.dispose();

	await page.goto('/login');
	await expect(page.locator('#password')).toBeVisible();
	// Считаем, сколько нажатий реально пришло в каждое поле.
	await page.evaluate(() => {
		const counters = { email: 0, password: 0 };
		Object.assign(window, { __keys: counters });
		document.querySelector('#email')!.addEventListener('keydown', () => (counters.email += 1));
		document.querySelector('#password')!.addEventListener('keydown', () => (counters.password += 1));
	});
	await page.locator('#email').click();
	await page.locator('#email').pressSequentially(email);
	await page.locator('#password').click();
	await page.locator('#password').pressSequentially(password);

	const signal = nextSignal(page, 'login');
	await page.locator('form').getByRole('button', { name: 'Войти', exact: true }).click();
	const { body, status } = await signal;
	expect(status).toBe(204);
	const keys = await page.evaluate(() => (window as unknown as { __keys: { email: number; password: number } }).__keys);
	expect(keys.password).toBe(password.length);
	expect(keys.email).toBeGreaterThan(0);

	// Динамика есть только для email: столько удержаний, сколько нажатий в email, ни одного от пароля.
	expect(body.typing).not.toBeNull();
	expect(body.typing!.dwell).toHaveLength(keys.email);
	expect(body.typing!.flight).toHaveLength(keys.email - 1);
	expect([...body.typing!.dwell, ...body.typing!.flight].every(Number.isInteger)).toBe(true);
	expect(Object.keys(body.typing!).sort()).toEqual(['dwell', 'flight', 'input']);
});
