import type { Locator, Page } from '@playwright/test';
import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, enrolledStudent, unique } from './support/api';
import { signIn, startAttempt } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

const FIRST = 'Первый вопрос: выберите «Верно»';
const SECOND = 'Второй вопрос: выберите «Верно»';
const THIRD = 'Третий вопрос: опишите манёвр';

/** Выделенный тройным щелчком текст. */
async function selectedAfterTripleClick(page: Page, target: Locator): Promise<string> {
	await target.click({ clickCount: 3 });
	return page.evaluate(() => window.getSelection()?.toString() ?? '');
}

/** Отменено ли событие копирования, пришедшее из элемента. */
function copyPrevented(target: Locator): Promise<boolean> {
	return target.evaluate((element) => {
		const event = new Event('copy', { bubbles: true, cancelable: true });
		element.dispatchEvent(event);
		return event.defaultPrevented;
	});
}

test('27. последовательный режим отдаёт только текущий вопрос, запрет копирования на прохождении и разборе', async ({ page, context, admin }) => {
	const { id: courseId } = await admin.api.call<{ id: string }>('POST', '/api/courses', {
		title: unique('Курс Защита'),
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.api.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: unique('Тест Защита')
	});
	for (const body of [FIRST, SECOND]) {
		await admin.api.call('POST', `/api/tests/${testId}/questions`, {
			kind: 'single',
			body,
			options: [
				{ body: 'Верно', is_correct: true },
				{ body: 'Неверно', is_correct: false }
			]
		});
	}
	await admin.api.call('POST', `/api/tests/${testId}/questions`, { kind: 'free_text', body: THIRD, options: [] });
	const detail = await admin.api.call<{ settings: Record<string, unknown> }>('GET', `/api/tests/${testId}`);
	await admin.api.call('PATCH', `/api/tests/${testId}`, {
		settings: { ...detail.settings, free_navigation: false, copy_policy: 'forbidden' }
	});
	await admin.api.call('POST', `/api/tests/${testId}/publish`);
	const stream = await addStream(admin.api, courseId, testId);
	const student = await enrolledStudent(stream, 'Студент Без Копирования');

	await signIn(context, page, student);
	const attemptId = await startAttempt(page, testId);
	await expect(page.getByText('Вопрос 1 из 3')).toBeVisible();

	// Следующих вопросов в ответе API нет, но их число известно.
	const view = await student.api.call<{ questions: Array<{ index: number }>; question_count: number }>(
		'GET',
		`/api/attempts/${attemptId}`
	);
	expect(view.questions.map((question) => question.index)).toEqual([0]);
	expect(view.question_count).toBe(3);
	expect(JSON.stringify(view)).not.toContain('Второй вопрос');

	// Текст вопроса не выделяется и не копируется.
	const first = page.getByText(FIRST);
	expect(await selectedAfterTripleClick(page, first)).toBe('');
	expect(await copyPrevented(first)).toBe(true);
	await expect(page.getByText('Копирование в этом тесте отключено')).toBeVisible();

	await page.getByLabel('Верно', { exact: true }).click();
	await page.getByRole('button', { name: 'Далее' }).click();
	await expect(page.getByText('Вопрос 2 из 3')).toBeVisible();
	await expect(page.getByText(SECOND)).toBeVisible();
	await expect(page.getByText(FIRST)).toHaveCount(0);

	await page.getByLabel('Верно', { exact: true }).click();
	await page.getByRole('button', { name: 'Далее' }).click();
	await expect(page.getByText('Вопрос 3 из 3')).toBeVisible();

	// Поле свободного ответа работает как обычно.
	const field = page.getByPlaceholder('Ваш ответ');
	await field.fill('Обгон слева');
	await field.click({ clickCount: 3 });
	const fieldSelection = await field.evaluate((element: HTMLTextAreaElement) =>
		element.value.slice(element.selectionStart, element.selectionEnd)
	);
	expect(fieldSelection).toBe('Обгон слева');
	expect(await copyPrevented(field)).toBe(false);
	// filter({ hasText: 'Сохранено' }) ищет без учёта регистра и совпал бы с «Не сохранено».
	await expect(page.getByRole('status').getByText('Сохранено', { exact: true })).toBeVisible();

	await page.getByRole('button', { name: 'Сдать попытку' }).click();
	await page.getByRole('dialog').getByRole('button', { name: 'Сдать', exact: true }).click();
	await page.waitForURL(/\/review$/);
	const reviewed = page.getByText(SECOND);
	await expect(reviewed).toBeVisible();
	expect(await selectedAfterTripleClick(page, reviewed)).toBe('');

	// Тест уже заблокирован, но запрет копирования методист снимает.
	const staff = await context.browser()!.newContext();
	const staffPage = await staff.newPage();
	await signIn(staff, staffPage, admin);
	await staffPage.goto(`/tests/${testId}`);
	const checkbox = staffPage.getByLabel('Запретить копирование текста');
	await expect(checkbox).toBeChecked();
	await checkbox.click();
	await staffPage.getByRole('button', { name: 'Сохранить настройки' }).click();
	await expect(staffPage.getByText('Настройки сохранены')).toBeVisible();
	await staffPage.reload();
	await expect(staffPage.getByLabel('Запретить копирование текста')).not.toBeChecked();
	await staff.close();

	await page.reload();
	await expect(reviewed).toBeVisible();
	expect(await selectedAfterTripleClick(page, reviewed)).toContain('Второй вопрос');
	await student.api.dispose();
});
