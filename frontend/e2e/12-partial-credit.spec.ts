import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, enrolledStudent, unique } from './support/api';
import { signIn, startAttempt } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('25. «балл за каждый верный»: методист выбирает режим, студент видит подсказку и частичный балл с процентом', async ({ page, context, admin, browser }) => {
	const { id: courseId } = await admin.api.call<{ id: string }>('POST', '/api/courses', {
		title: unique('Курс Частичный'),
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.api.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: unique('Тест Частичный')
	});

	// Методист: режим выбирается в списке типа в начале формы, максимум пересчитывается по верным.
	await signIn(context, page, admin);
	await page.goto(`/tests/${testId}`);
	const editor = page.locator('form', { has: page.getByRole('button', { name: 'Сохранить вопрос' }) });
	await page.getByRole('button', { name: 'Добавить вопрос' }).click();
	await editor.getByLabel('Тип вопроса').selectOption({ label: 'Несколько: балл за каждый верный' });
	await editor.getByLabel('Текст вопроса').fill('Что берём на поиск?');
	await editor.getByRole('button', { name: 'Добавить вариант' }).click();
	await editor.getByLabel('Текст варианта 1').fill('Фонарь');
	await editor.getByLabel('Текст варианта 2').fill('Рация');
	await editor.getByLabel('Текст варианта 3').fill('Гитара');
	const max = editor.getByTestId('question-max');
	await editor.getByText('Верный', { exact: true }).nth(0).click();
	await expect(max).toHaveText('макс. за вопрос: 1');
	await editor.getByText('Верный', { exact: true }).nth(1).click();
	await expect(max).toHaveText('макс. за вопрос: 2');
	await editor.getByRole('button', { name: 'Сохранить вопрос' }).click();
	await expect(page.getByRole('button', { name: 'Сохранить вопрос' })).toHaveCount(0);
	await expect(page.getByText('несколько верных, «балл за каждый»')).toBeVisible();
	await page.getByRole('button', { name: 'Опубликовать' }).click();
	await expect(page.getByText('опубликован', { exact: true }).first()).toBeVisible();

	// Студент: подсказка о частичном начислении, один верный из двух — 1 из 2, 50 %.
	const stream = await addStream(admin.api, courseId, testId);
	const student = await enrolledStudent(stream, unique('Студент Частичный'));
	const studentContext = await browser.newContext();
	const studentPage = await studentContext.newPage();
	await signIn(studentContext, studentPage, student);
	await startAttempt(studentPage, testId);
	await expect(studentPage.getByText(/Засчитывается частично/)).toBeVisible();
	await studentPage.getByLabel('Фонарь', { exact: true }).click();
	await expect(studentPage.getByRole('status').filter({ hasText: 'Сохранено' })).toBeVisible();
	await studentPage.getByRole('button', { name: 'Сдать попытку' }).click();
	await studentPage.getByRole('dialog').getByRole('button', { name: 'Сдать', exact: true }).click();
	await studentPage.waitForURL(/\/review$/);
	await expect(studentPage.getByText('1 из 2', { exact: true })).toBeVisible();
	await expect(studentPage.getByText('· 50 %')).toBeVisible();
	await studentContext.close();

	// Преподаватель видит процент в таблице результатов.
	await page.goto(`/tests/${testId}/results`);
	await expect(page.locator('table tbody tr').first()).toContainText('50 %');
	await student.api.dispose();
});
