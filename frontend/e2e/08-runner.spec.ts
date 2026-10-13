import { fileURLToPath } from 'node:url';
import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, enrolledStudent, seedTest } from './support/api';
import { choose, signIn, startAttempt, timerSeconds } from './support/browser';

const FIGURE = fileURLToPath(new URL('./fixtures/figure.png', import.meta.url));

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

interface AttemptState {
	submitted_at: string | null;
	answers: Array<{ question_id: string; selected_option_ids: string[] }>;
	questions: Array<{ id: string; index: number; options: Array<{ id: string; body: string }> }>;
}

test('16. обрыв связи: ответы копятся на устройстве и сохраняются, когда сеть вернулась', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { freeNavigation: true, timeLimitMinutes: 30 });
	const student = await enrolledStudent(seeded, 'Студент Без Сети');
	await signIn(context, page, student);
	const attemptId = await startAttempt(page, seeded.testId);

	await context.setOffline(true);
	await page.getByLabel('Верно', { exact: true }).click();
	const status = page.getByRole('status').filter({ hasText: 'Не сохранено' });
	await expect(status).toBeVisible();
	const banner = page.getByText(/Есть несохранённые ответы \(\d+\)/);
	await expect(banner).toHaveText('Есть несохранённые ответы (1)');
	await expect(page.getByText(/Нет связи с интернетом/)).toBeVisible();

	await page.getByRole('button', { name: /^Вопрос 2/ }).click();
	await expect(page.getByText('Вопрос 2 из 2')).toBeVisible();
	await page.getByLabel('Неверно', { exact: true }).click();
	await expect(banner).toHaveText('Есть несохранённые ответы (2)');

	// Пока сети нет, на сервер ничего не дошло (API-клиент теста работает вне браузера).
	const offline = await student.api.call<AttemptState>('GET', `/api/attempts/${attemptId}`);
	expect(offline.answers.filter((answer) => answer.selected_option_ids.length > 0)).toHaveLength(0);

	await context.setOffline(false);
	await expect(banner).toHaveCount(0, { timeout: 15_000 });
	await expect(page.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();

	// Оба ответа доходят до сервера после восстановления связи (очередь досылает их сама).
	const expected = [...offline.questions]
		.sort((a, b) => a.index - b.index)
		.map((question, index) => ({
			question: question.id,
			options: [question.options.find((o) => o.body === (index === 0 ? 'Верно' : 'Неверно'))!.id]
		}));
	await expect
		.poll(
			async () => {
				const online = await student.api.call<AttemptState>('GET', `/api/attempts/${attemptId}`);
				const saved = new Map(online.answers.map((answer) => [answer.question_id, answer.selected_option_ids]));
				return expected.map((item) => ({ question: item.question, options: saved.get(item.question) ?? [] }));
			},
			{ message: 'ответы, выбранные без сети, сохранены на сервере', timeout: 30_000 }
		)
		.toEqual(expected);
	await student.api.dispose();
});

test('17. автосдача по таймеру: лимит 1 минута, попытка сдана «по времени» с сохранённым ответом', async ({ page, context, admin }) => {
	// Лимит задаётся в целых минутах, а срок считает сервер, — ускорить нечем: честно ждём минуту.
	test.setTimeout(150_000);
	const seeded = await seedTest(admin.api, { timeLimitMinutes: 1, freeNavigation: true });
	const student = await enrolledStudent(seeded, 'Студент Не Успел');
	await signIn(context, page, student);
	const attemptId = await startAttempt(page, seeded.testId);
	await choose(page, 'Верно');
	const left = timerSeconds(await page.getByRole('timer').innerText());
	expect(left).toBeLessThanOrEqual(60);

	// Кнопку «Сдать» не нажимаем: раннер сам сдаёт попытку, когда таймер дошёл до нуля.
	await page.waitForURL(/\/review$/, { timeout: (left + 30) * 1000 });
	await expect(page.getByText('Балл за попытку')).toBeVisible();
	await expect(page.locator('main')).toContainText('сдана по времени');
	await expect(page.locator('main')).toContainText(/1\s*из\s*2/);

	const review = await student.api.call<{ submit_reason: string | null }>('GET', `/api/attempts/${attemptId}/review`);
	expect(review.submit_reason).toBe('timeout');
	await student.api.dispose();
});

test.describe('мобильный экран 375×812', () => {
	test.use({ viewport: { width: 375, height: 812 }, isMobile: true, hasTouch: true });

	test('18. студент проходит тест и открывает разбор без горизонтальной прокрутки', async ({ page, context, admin }) => {
		const seeded = await seedTest(admin.api, { freeNavigation: false, withImage: FIGURE });
		const student = await enrolledStudent(seeded, 'Студент С Телефона');
		await signIn(context, page, student);

		const noHorizontalScroll = async (where: string) => {
			const size = await page.evaluate(() => ({
				scroll: document.documentElement.scrollWidth,
				inner: window.innerWidth
			}));
			expect(size.scroll, `${where}: scrollWidth ${size.scroll} > innerWidth ${size.inner}`).toBeLessThanOrEqual(size.inner);
		};

		// Панель студента
		await page.goto('/');
		const available = page.locator('section', { has: page.getByRole('heading', { name: 'Доступные сейчас' }) });
		const card = available.locator('[data-slot=card]', { hasText: seeded.testTitle });
		await expect(card).toBeVisible();
		await noHorizontalScroll('панель');
		await card.getByRole('link', { name: 'Начать' }).click();
		await expect(page.getByRole('button', { name: 'Начать', exact: true })).toBeVisible();
		await noHorizontalScroll('старт');
		await page.getByRole('button', { name: 'Начать', exact: true }).click();
		await page.waitForURL(/\/attempts\/[0-9a-f-]{36}$/);

		// Раннер: вопрос с картинкой, последовательный режим.
		await expect(page.getByText('Вопрос 1 из 2')).toBeVisible();
		await expect(page.locator('main img[src^="/api/files/"]')).toBeVisible();
		await noHorizontalScroll('вопрос 1');
		await choose(page, 'Верно');
		await page.getByRole('button', { name: 'Далее' }).click();
		await expect(page.getByText('Вопрос 2 из 2')).toBeVisible();
		await choose(page, 'Верно');
		await noHorizontalScroll('вопрос 2');
		await page.getByRole('button', { name: 'Сдать попытку' }).click();
		await page.getByRole('dialog').getByRole('button', { name: 'Сдать', exact: true }).click();

		// Разбор
		await page.waitForURL(/\/attempts\/[0-9a-f-]{36}\/review$/);
		await expect(page.getByText('Балл за попытку')).toBeVisible();
		await expect(page.locator('main')).toContainText(/2\s*из\s*2/);
		await noHorizontalScroll('разбор');
		await student.api.dispose();
	});
});
