import { fileURLToPath } from 'node:url';
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
