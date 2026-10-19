import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, enrolledStudent, registerUser, seedTest } from './support/api';
import { signIn } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

interface RunnerAttempt {
	questions: Array<{ id: string; index: number; options: Array<{ id: string; body: string }> }>;
}

test('7. результаты: строка студента, CSV с BOM и Excel', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { freeNavigation: true });
	const student = await enrolledStudent(seeded, unique7());

	// Студент сдаёт попытку через API: верно отвечает на первый вопрос.
	const { attempt_id: attemptId } = await student.api.call<{ attempt_id: string }>(
		'POST',
		`/api/tests/${seeded.testId}/attempts`
	);
	const attempt = await student.api.call<RunnerAttempt>('GET', `/api/attempts/${attemptId}`);
	const first = [...attempt.questions].sort((a, b) => a.index - b.index)[0]!;
	const right = first.options.find((option) => option.body === 'Верно')!;
	await student.api.call('PUT', `/api/attempts/${attemptId}/answers/${first.id}`, {
		selected_option_ids: [right.id],
		text: null,
		client_seq: Date.now()
	});
	await student.api.call('POST', `/api/attempts/${attemptId}/submit`);

	await signIn(context, page, admin);
	await page.goto(`/tests/${seeded.testId}/results`);
	await expect(page.getByRole('row', { name: new RegExp(student.user.display_name) })).toBeVisible();

	const csvLink = page.getByRole('link', { name: /Скачать CSV/ });
	const href = await csvLink.getAttribute('href');
	expect(href).toMatch(new RegExp(`^/api/tests/${seeded.testId}/results\\.csv`));
	const csv = await admin.api.raw('GET', href!);
	expect(csv.status()).toBe(200);
	const body = await csv.body();
	expect([...body.subarray(0, 3)]).toEqual([0xef, 0xbb, 0xbf]);
	expect(body.toString('utf8')).toContain(student.user.email);

	const xlsxHref = await page.getByRole('link', { name: /Скачать Excel/ }).getAttribute('href');
	expect(xlsxHref).toMatch(new RegExp(`^/api/tests/${seeded.testId}/results\\.xlsx`));
	const xlsx = await admin.api.raw('GET', xlsxHref!);
	expect(xlsx.status()).toBe(200);
	expect(xlsx.headers()['content-type']).toContain('spreadsheetml');
	expect((await xlsx.body()).subarray(0, 2).toString('latin1')).toBe('PK');
	await student.api.dispose();
});

test('8. суперпользователь выдаёт роль методиста — у пользователя появляется «Курсы»', async ({ browser, page, context, admin }) => {
	const user = await registerUser('methodist', 'Будущий Методист');

	// До выдачи роли пункта «Курсы» нет.
	const userContext = await browser.newContext();
	const userPage = await userContext.newPage();
	await signIn(userContext, userPage, user);
	const nav = userPage.getByRole('navigation', { name: 'Основная навигация' });
	await expect(nav.getByRole('link', { name: 'Мои тесты' })).toBeVisible();
	await expect(nav.getByRole('link', { name: 'Курсы' })).toHaveCount(0);

	// Суперпользователь в /admin ставит галочку «Методист».
	await signIn(context, page, admin);
	await page.goto('/admin');
	await page.getByLabel('Поиск пользователей').fill(user.email);
	const checkbox = page.locator(`[id="role-${user.user.id}-methodist"]`);
	await expect(checkbox).toBeVisible();
	await expect(checkbox).not.toBeChecked();
	await checkbox.click();
	await expect(checkbox).toBeChecked();
	await expect
		.poll(async () => {
			const users = await admin.api.call<Array<{ id: string; roles: string[] }>>(
				'GET',
				`/api/admin/users?search=${encodeURIComponent(user.email)}`
			);
			return users.find((item) => item.id === user.user.id)?.roles ?? [];
		})
		.toContain('methodist');

	// После перезагрузки у пользователя появляется «Курсы».
	await userPage.reload();
	await expect(nav.getByRole('link', { name: 'Курсы' })).toBeVisible();
	await userContext.close();
	await user.api.dispose();
});

function unique7(): string {
	return `Студент Результаты ${Date.now().toString(36)}`;
}
