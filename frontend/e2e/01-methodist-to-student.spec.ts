import { fileURLToPath } from 'node:url';
import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, newPassword, tokenFromUrl, unique, uniqueEmail } from './support/api';
import { idFromUrl, signIn } from './support/browser';

const FIGURE = fileURLToPath(new URL('./fixtures/figure.png', import.meta.url));

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

/** Данные, которые сценарий 1 передаёт сценарию 2. */
const shared = { courseTitle: '', testTitle: '', testId: '', inviteToken: '' };

test.describe.serial('Методист готовит тест, студент проходит его по приглашению', () => {
	test('1. методист: курс, тест с картинкой, публикация, поток, приглашение, окно', async ({ page, context, admin }) => {
		shared.courseTitle = unique('Курс E2E');
		shared.testTitle = unique('Контрольная E2E');
		const streamName = unique('Поток E2E');
		await signIn(context, page, admin);

		// Курс
		await page.goto('/courses');
		await page.getByRole('button', { name: 'Создать курс' }).click();
		await page.locator('#course-title').fill(shared.courseTitle);
		await page.getByRole('dialog').getByRole('button', { name: 'Создать', exact: true }).click();
		await page.waitForURL(/\/courses\/[0-9a-f-]{36}$/);
		await expect(page.getByRole('heading', { name: shared.courseTitle })).toBeVisible();

		// Тест
		await page.getByRole('button', { name: 'Создать тест' }).click();
		await page.locator('#test-title-new').fill(shared.testTitle);
		await page.getByRole('button', { name: 'Создать и открыть' }).click();
		await page.waitForURL(/\/tests\/[0-9a-f-]{36}$/);
		shared.testId = idFromUrl(page, 'tests');

		// Открытая форма вопроса (сохранённые вопросы показываются отдельно).
		const editor = page.locator('form', { has: page.getByRole('button', { name: 'Сохранить вопрос' }) });

		// Вопрос 1: одиночный выбор без картинки
		await page.getByRole('button', { name: 'Добавить вопрос' }).click();
		await editor.getByLabel('Текст вопроса').fill('Сколько будет $2 + 2$?');
		await editor.getByLabel('Текст варианта 1').fill('4');
		await editor.getByLabel('Текст варианта 2').fill('5');
		await editor.getByText('Верный', { exact: true }).first().click();
		await editor.getByRole('button', { name: 'Сохранить вопрос' }).click();
		await expect(page.getByRole('button', { name: 'Сохранить вопрос' })).toHaveCount(0);

		// Вопрос 2: с картинкой из фикстуры
		await page.getByRole('button', { name: 'Добавить вопрос' }).click();
		await editor.getByLabel('Текст вопроса').fill('Какого цвета верхний левый угол рисунка?');
		await editor.locator('input[type=file]').first().setInputFiles(FIGURE);
		await expect(editor.locator('img[src^="/api/files/"]').first()).toBeVisible();
		await editor.getByLabel('Описание картинки').first().fill('Градиент от синего к фиолетовому');
		await editor.getByLabel('Текст варианта 1').fill('Синий');
		await editor.getByLabel('Текст варианта 2').fill('Красный');
		await editor.getByText('Верный', { exact: true }).first().click();
		await editor.getByRole('button', { name: 'Сохранить вопрос' }).click();
		await expect(page.getByRole('button', { name: 'Сохранить вопрос' })).toHaveCount(0);
		await expect(page.getByText('2 вопроса')).toBeVisible();

		// Публикация
		await page.getByRole('button', { name: 'Опубликовать' }).click();
		await expect(page.getByText('опубликован', { exact: true }).first()).toBeVisible();

		// Поток и назначение курса
		await page.goto('/streams');
		await page.getByRole('button', { name: 'Создать поток' }).click();
		await page.locator('#stream-name').fill(streamName);
		await page.getByRole('button', { name: 'Создать', exact: true }).click();
		await page.waitForURL(/\/streams\/[0-9a-f-]{36}$/);
		await page.getByRole('tab', { name: 'Курсы' }).click();
		await page.locator('#assign-course').click();
		await page.getByRole('option', { name: shared.courseTitle }).click();
		await page.getByRole('button', { name: 'Назначить', exact: true }).click();
		await expect(page.getByRole('link', { name: shared.courseTitle })).toBeVisible();

		// Ссылка-приглашение: полный адрес показывается один раз
		await page.getByRole('tab', { name: 'Ссылки' }).click();
		await page.locator('#invite-label').fill('Сквозной тест');
		await page.getByRole('button', { name: 'Создать ссылку' }).click();
		const inviteInput = page.locator('input[readonly]').first();
		await expect(inviteInput).toHaveValue(/\/invite\//);
		shared.inviteToken = tokenFromUrl(await inviteInput.inputValue());

		// Окно прохождения
		await page.goto(`/tests/${shared.testId}`);
		const row = page.locator('li', { hasText: streamName });
		await row.getByRole('button', { name: 'Открыть сейчас' }).click();
		await expect(row.getByText('открыто', { exact: true })).toBeVisible();
	});

	test('2. студент регистрируется по приглашению, проходит тест и видит 1 / 2', async ({ browser }) => {
		test.skip(!shared.inviteToken, 'Сценарий 1 не выдал приглашение');
		const context = await browser.newContext();
		const page = await context.newPage();

		await page.goto(`/invite/${shared.inviteToken}`);
		await page.locator('#email').fill(uniqueEmail('invite'));
		await page.locator('#name').fill('Студент По Приглашению');
		const password = newPassword();
		await page.locator('#password').fill(password);
		await page.locator('#repeat').fill(password);
		await page.getByRole('button', { name: 'Зарегистрироваться и вступить' }).click();
		await page.waitForURL((url) => url.pathname === '/');

		const available = page.locator('section', { has: page.getByRole('heading', { name: 'Доступные сейчас' }) });
		const card = available.locator('[data-slot=card]', { hasText: shared.testTitle });
		await expect(card).toBeVisible();
		await card.getByRole('link', { name: 'Начать' }).click();
		await page.getByRole('button', { name: 'Начать', exact: true }).click();
		await page.waitForURL(/\/attempts\/[0-9a-f-]{36}$/);

		// По умолчанию навигация последовательная: ответ, «Далее», ответ, «Сдать попытку».
		await page.getByLabel('4', { exact: true }).click();
		await expect(page.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
		await page.getByRole('button', { name: 'Далее' }).click();
		await expect(page.getByText('Вопрос 2 из 2')).toBeVisible();
		await expect(page.locator('img[src^="/api/files/"]')).toBeVisible();
		await page.getByLabel('Красный', { exact: true }).click(); // неверно
		await expect(page.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
		await page.getByRole('button', { name: 'Сдать попытку' }).click();
		await page.getByRole('dialog').getByRole('button', { name: 'Сдать', exact: true }).click();

		await page.waitForURL(/\/attempts\/[0-9a-f-]{36}\/review$/);
		await expect(page.getByText('Балл за попытку')).toBeVisible();
		await expect(page.locator('main')).toContainText(/1\s*из\s*2/);
		await context.close();
	});
});
