import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, registerUser, seedCourseTest, tokenFromUrl } from './support/api';
import { signIn } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('15. преподаватель создаёт ссылку, открывает окно и видит журнал переходов', async ({ page, context, admin }) => {
	const course = await seedCourseTest(admin.api);
	const teacher = await registerUser('teacher', 'Преподаватель Курса');
	expect(teacher.user.roles.superuser).toBe(false);
	expect(teacher.user.roles.methodist).toBe(false);
	await admin.api.call('POST', `/api/courses/${course.courseId}/teachers`, { email: teacher.email });
	// Поток с курсом готовит методист; ссылкой и окном управляет преподаватель.
	const { streamId, streamName } = await addStream(admin.api, course.courseId, course.testId, { inviteAndOpen: false });

	await signIn(context, page, teacher);

	// Ссылка-приглашение
	await page.goto(`/streams/${streamId}`);
	await page.getByRole('tab', { name: 'Ссылки' }).click();
	await page.locator('#invite-label').fill('Ссылка преподавателя');
	await page.getByRole('button', { name: 'Создать ссылку' }).click();
	const inviteInput = page.locator('input[readonly]').first();
	await expect(inviteInput).toHaveValue(/\/invite\//);
	const token = tokenFromUrl(await inviteInput.inputValue());

	// Окно прохождения
	await page.goto(`/tests/${course.testId}`);
	const row = page.locator('li', { hasText: streamName });
	await row.getByRole('button', { name: 'Открыть сейчас' }).click();
	await expect(row.getByText('открыто', { exact: true })).toBeVisible();

	// Переходы: первый студент — один раз, второй — дважды (повторный переход тоже в журнале).
	const first = await registerUser('journal', 'Студент Один Переход');
	const second = await registerUser('journal', 'Студент Два Перехода');
	await first.api.call('POST', `/api/invites/${token}/join`);
	await second.api.call('POST', `/api/invites/${token}/join`);
	await second.api.call('POST', `/api/invites/${token}/join`);

	await page.goto(`/streams/${streamId}`);
	await page.getByRole('tab', { name: 'Журнал' }).click();
	const journal = page.getByRole('tabpanel').locator('table tbody tr');
	await expect(journal).toHaveCount(3);
	await expect(journal.filter({ hasText: first.email })).toHaveCount(1);
	await expect(journal.filter({ hasText: second.email })).toHaveCount(2);
	await expect(journal.first()).toContainText('Ссылка преподавателя');

	// Фильтр по студенту
	await page.locator('#filter-user').click();
	await page.getByRole('option', { name: new RegExp(second.email.replace(/[.+]/g, '\\$&')) }).click();
	await expect(journal).toHaveCount(2);
	await expect(journal.filter({ hasText: first.email })).toHaveCount(0);

	for (const account of [teacher, first, second]) await account.api.dispose();
});
