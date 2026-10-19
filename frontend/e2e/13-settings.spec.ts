import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, unique } from './support/api';
import { signIn } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('26. настройки теста: лимит времени, попытки и показ результатов сохраняются после перезагрузки', async ({ page, context, admin }) => {
	const { id: courseId } = await admin.api.call<{ id: string }>('POST', '/api/courses', {
		title: unique('Курс Настройки'),
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.api.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: unique('Тест Настройки')
	});

	await signIn(context, page, admin);
	await page.goto(`/tests/${testId}`);
	const save = page.getByRole('button', { name: 'Сохранить настройки' });

	// Снятая галка с пустым полем — ошибка; введённое число её убирает.
	await page.getByLabel('без лимита').click();
	await expect(page.getByText('Лимит времени — целое число минут от 1.')).toBeVisible();
	await page.getByLabel('Лимит времени на попытку').fill('45');
	await expect(page.getByText('Лимит времени — целое число минут от 1.')).toHaveCount(0);
	await page.getByLabel('Попыток на студента').fill('2');
	await page.getByLabel('Показ результатов студенту').selectOption('never');
	await expect(save).toBeEnabled();
	await save.click();
	await expect(page.getByText('Настройки сохранены')).toBeVisible();

	await page.reload();
	await expect(page.getByLabel('Лимит времени на попытку')).toHaveValue('45');
	await expect(page.getByLabel('без лимита')).not.toBeChecked();
	await expect(page.getByLabel('Попыток на студента')).toHaveValue('2');
	await expect(page.getByLabel('Показ результатов студенту')).toHaveValue('never');
	const saved = await admin.api.call<{ settings: { time_limit_minutes: number | null; attempts_limit: number | null; review_policy: string } }>(
		'GET',
		`/api/tests/${testId}`
	);
	expect(saved.settings).toMatchObject({ time_limit_minutes: 45, attempts_limit: 2, review_policy: 'never' });

	// Меняется только показ результатов — сохраняется он, остальное остаётся.
	await page.getByLabel('Показ результатов студенту').selectOption('after_close');
	await save.click();
	await expect(page.getByText('Настройки сохранены')).toBeVisible();
	await page.reload();
	await expect(page.getByLabel('Показ результатов студенту')).toHaveValue('after_close');
	await expect(page.getByLabel('Лимит времени на попытку')).toHaveValue('45');
});
