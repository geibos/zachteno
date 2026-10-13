import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, enrolledStudent, seedTest } from './support/api';
import { choose, signIn, startAttempt, timerSeconds } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('3. таймер виден и убывает, после сдачи сразу виден разбор', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { timeLimitMinutes: 1, freeNavigation: true });
	const student = await enrolledStudent(seeded, 'Студент Таймер');
	await signIn(context, page, student);
	await startAttempt(page, seeded.testId);

	const timer = page.getByRole('timer');
	await expect(timer).toBeVisible();
	const first = timerSeconds(await timer.innerText());
	expect(first).toBeGreaterThan(30);
	expect(first).toBeLessThanOrEqual(60);
	await expect.poll(async () => timerSeconds(await timer.innerText()), { timeout: 5_000 }).toBeLessThan(first);

	// Автосдачу по времени проверяют бэкенд-тесты; здесь — ручная сдача без ответов.
	await page.getByRole('button', { name: 'Сдать', exact: true }).first().click();
	await page.getByRole('dialog').getByRole('button', { name: 'Сдать с пропусками' }).click();
	await page.waitForURL(/\/review$/);
	await expect(page.getByText('Балл за попытку')).toBeVisible();
	await student.api.dispose();
});

test('4. после закрытия окна начатая попытка дорабатывается, новую начать нельзя', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { timeLimitMinutes: 30, freeNavigation: true, attemptsLimit: null });
	const student = await enrolledStudent(seeded, 'Студент Дорабатывает');
	await signIn(context, page, student);
	await startAttempt(page, seeded.testId);
	await choose(page, 'Верно');

	await admin.api.call('PUT', `/api/tests/${seeded.testId}/windows/${seeded.streamId}`, { action: 'close' });

	// Ответ после закрытия окна сохраняется: срок попытки окно не меняет.
	await page.getByRole('button', { name: /^Вопрос 2/ }).click();
	await expect(page.getByText('Вопрос 2 из 2')).toBeVisible();
	await choose(page, 'Верно');
	await expect(page.getByText('Не удалось сохранить')).toHaveCount(0);

	await page.getByRole('button', { name: 'Сдать', exact: true }).first().click();
	await page.getByRole('dialog').getByRole('button', { name: 'Сдать', exact: true }).click();
	await page.waitForURL(/\/review$/);

	// Новую попытку не начать: и через API, и в интерфейсе.
	const retry = await student.api.raw('POST', `/api/tests/${seeded.testId}/attempts`);
	expect(retry.status()).toBe(409);
	await page.goto(`/tests/${seeded.testId}/start`);
	await expect(page.locator('main')).toContainText(/закрыт|недоступен/);
	await expect(page.getByRole('button', { name: 'Начать', exact: true })).toHaveCount(0);
	await student.api.dispose();
});

test('5. последовательный режим: «Далее» только с ответом, назад нельзя', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { freeNavigation: false });
	const student = await enrolledStudent(seeded, 'Студент По Порядку');
	await signIn(context, page, student);
	const attemptId = await startAttempt(page, seeded.testId);

	const next = page.getByRole('button', { name: 'Далее' });
	await expect(next).toBeDisabled();
	await expect(page.getByRole('navigation', { name: 'Вопросы' })).toHaveCount(0);
	await choose(page, 'Верно');
	await expect(next).toBeEnabled();
	await next.click();
	await expect(page.getByText('Вопрос 2 из 2')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Назад' })).toHaveCount(0);

	// Прямой запрос к API тоже не даёт изменить ответ на пройденный вопрос.
	const attempt = await student.api.call<{ questions: Array<{ id: string; index: number }> }>(
		'GET',
		`/api/attempts/${attemptId}`
	);
	const firstQuestion = [...attempt.questions].sort((a, b) => a.index - b.index)[0];
	expect(firstQuestion).toBeDefined();
	const back = await student.api.raw('PUT', `/api/attempts/${attemptId}/answers/${firstQuestion!.id}`, {
		selected_option_ids: [],
		text: null,
		client_seq: Date.now()
	});
	expect(back.status()).toBe(409);
	await student.api.dispose();
});

test('6. свободный режим: пропуск вопроса → предупреждение со списком неотвеченных', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { questions: 3, freeNavigation: true });
	const student = await enrolledStudent(seeded, 'Студент Пропускает');
	await signIn(context, page, student);
	await startAttempt(page, seeded.testId);
	await choose(page, 'Верно');

	await page.getByRole('button', { name: 'Сдать', exact: true }).first().click();
	const dialog = page.getByRole('dialog');
	await expect(dialog).toContainText('Без ответа 2 вопроса');
	await expect(dialog.getByRole('button', { name: '№ 2' })).toBeVisible();
	await expect(dialog.getByRole('button', { name: '№ 3' })).toBeVisible();
	await expect(dialog.getByRole('button', { name: 'Сдать с пропусками' })).toBeVisible();

	// Ссылка из предупреждения ведёт к вопросу.
	await dialog.getByRole('button', { name: '№ 3' }).click();
	await expect(page.getByText('Вопрос 3 из 3')).toBeVisible();
	await student.api.dispose();
});
