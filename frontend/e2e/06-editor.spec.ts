import { fileURLToPath } from 'node:url';
import type { Locator, Page } from '@playwright/test';
import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, enrolledStudent, unique } from './support/api';
import { signIn, startAttempt } from './support/browser';

const FIGURE = fileURLToPath(new URL('./fixtures/figure.png', import.meta.url));

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('14. редактор: формула KaTeX в вопросе и вариант-картинка; студент видит их в раннере', async ({ page, context, admin, browser }) => {
	const { id: courseId } = await admin.api.call<{ id: string }>('POST', '/api/courses', {
		title: unique('Курс Формулы'),
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.api.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: unique('Тест Формулы')
	});

	await signIn(context, page, admin);
	await page.goto(`/tests/${testId}`);
	const editor = page.locator('form', { has: page.getByRole('button', { name: 'Сохранить вопрос' }) });
	await page.getByRole('button', { name: 'Добавить вопрос' }).click();
	await editor.getByLabel('Текст вопроса').fill('Какая фигура задаётся уравнением $x^2 + y^2 = r^2$?');
	// Живой предпросмотр рисует формулу KaTeX.
	await expect(editor.locator('.katex').first()).toBeVisible();

	// Вариант 1 — картинка с подписью, вариант 2 — только текст.
	await editor.getByLabel('Текст варианта 1').fill('Окружность');
	await editor.locator('input[type=file][id^="oimg-"]').first().setInputFiles(FIGURE);
	const optionImage = editor.locator('img[src^="/api/files/"]').first();
	await expect(optionImage).toBeVisible();
	await editor.getByLabel('Описание картинки').first().fill('Рисунок окружности');
	await editor.getByLabel('Текст варианта 2').fill('Парабола');
	await editor.getByText('Верный', { exact: true }).first().click();
	await editor.getByRole('button', { name: 'Сохранить вопрос' }).click();
	await expect(page.getByRole('button', { name: 'Сохранить вопрос' })).toHaveCount(0);
	await expect(page.getByText('1 вопрос', { exact: true })).toBeVisible();

	await page.getByRole('button', { name: 'Опубликовать' }).click();
	await expect(page.getByText('опубликован', { exact: true }).first()).toBeVisible();

	// Студент в раннере: формула отрисована, картинка у варианта загружена.
	const stream = await addStream(admin.api, courseId, testId);
	const student = await enrolledStudent(stream, 'Студент Формулы');
	const studentContext = await browser.newContext();
	const studentPage = await studentContext.newPage();
	await signIn(studentContext, studentPage, student);
	await startAttempt(studentPage, testId);
	const main = studentPage.locator('main');
	await expect(main.locator('.katex').first()).toBeVisible();
	await expect(main).not.toContainText('$x^2');
	const option = main.locator('label', { hasText: 'Окружность' });
	const picture = option.locator('img[alt="Рисунок окружности"]');
	await expect(picture).toBeVisible();
	await expect.poll(() => picture.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
	await option.click();
	await expect(studentPage.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
	await studentContext.close();
	await student.api.dispose();
});

// Полезная нагрузка методиста: каждый фрагмент при исполнении ставит флаг и открывает alert.
const SCRIPT = `<script>window.__pwned = 'script'; alert('script')</script>`;
const IMG = `<img src=x onerror="window.__pwned = 'img'; alert('img')">`;
const LINK = `[подробнее](javascript:window.__pwned='link';alert('link'))`;

/** На странице нет ни одного исполняемого следа полезной нагрузки, а сама она видна как текст. */
async function expectInert(page: Page, scope: Locator, fragments: string[]): Promise<void> {
	for (const fragment of fragments) await expect(scope).toContainText(fragment);
	await expect(page.locator('.md-content script')).toHaveCount(0);
	await expect(page.locator('img[src="x"], [onerror]')).toHaveCount(0);
	await expect(page.locator('a[href^="javascript:" i]')).toHaveCount(0);
	expect(await page.evaluate(() => (window as { __pwned?: unknown }).__pwned)).toBeUndefined();
}

test('22. попытка внедрить скрипт: <script>, onerror и javascript:-ссылка не исполняются ни в редакторе, ни у студента', async ({ page, context, admin, browser }) => {
	const dialogs: string[] = [];
	const watch = (target: Page): void => {
		target.on('dialog', (dialog) => {
			dialogs.push(dialog.message());
			void dialog.dismiss();
		});
	};
	watch(page);

	const { id: courseId } = await admin.api.call<{ id: string }>('POST', '/api/courses', {
		title: unique('Курс Внедрение'),
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.api.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: unique('Тест Внедрение')
	});

	const body = `Безопасен ли такой текст? ${SCRIPT} ${IMG} ${LINK}`;
	const safe = `Экранирован ${IMG}`;
	const unsafe = `Исполнен ${SCRIPT} ${LINK}`;

	await signIn(context, page, admin);
	await page.goto(`/tests/${testId}`);
	const editor = page.locator('form', { has: page.getByRole('button', { name: 'Сохранить вопрос' }) });
	await page.getByRole('button', { name: 'Добавить вопрос' }).click();
	await editor.getByLabel('Текст вопроса').fill(body);
	await editor.getByLabel('Текст варианта 1').fill(safe);
	await editor.getByLabel('Текст варианта 2').fill(unsafe);
	await editor.getByText('Верный', { exact: true }).first().click();

	// Живой предпросмотр: теги показаны текстом, в DOM их нет.
	await expectInert(page, editor, [SCRIPT, IMG, LINK]);

	await editor.getByRole('button', { name: 'Сохранить вопрос' }).click();
	await expect(page.getByRole('button', { name: 'Сохранить вопрос' })).toHaveCount(0);
	await expect(page.getByText('1 вопрос', { exact: true })).toBeVisible();

	// Сохранённый вопрос после перезагрузки — текст пришёл с сервера.
	await page.reload();
	const main = page.locator('main');
	await expectInert(page, main, [SCRIPT, IMG, LINK, 'Экранирован', 'Исполнен']);

	await page.getByRole('button', { name: 'Опубликовать' }).click();
	await expect(page.getByText('опубликован', { exact: true }).first()).toBeVisible();

	// Студент в раннере: то же самое, и ответ на вариант с полезной нагрузкой сохраняется.
	const stream = await addStream(admin.api, courseId, testId);
	const student = await enrolledStudent(stream, 'Студент Внедрение');
	const studentContext = await browser.newContext();
	const studentPage = await studentContext.newPage();
	watch(studentPage);
	await signIn(studentContext, studentPage, student);
	await startAttempt(studentPage, testId);
	const runner = studentPage.locator('main');
	await expectInert(studentPage, runner, [SCRIPT, IMG, LINK]);
	await runner.locator('label', { hasText: 'Экранирован' }).click();
	await expect(studentPage.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
	await expectInert(studentPage, runner, [SCRIPT, IMG]);

	expect(dialogs, 'ни один alert не открылся').toEqual([]);
	await studentContext.close();
	await student.api.dispose();
});
