import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, registerUser, seedCourseTest, uniqueEmail } from './support/api';
import { signIn } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('23. ручное добавление в поток, закрытый тест у студента и ссылка для студентов', async ({ browser, admin }) => {
	const course = await seedCourseTest(admin.api);
	const stream = await addStream(admin.api, course.courseId, course.testId, { inviteAndOpen: false });
	const student = await registerUser('manual', 'Студент Без Ссылки');
	const ghost = uniqueEmail('ghost');

	// Менеджер добавляет студента и незнакомый адрес списком.
	const staff = await browser.newContext();
	const staffPage = await staff.newPage();
	await signIn(staff, staffPage, admin);
	await staffPage.goto(`/streams/${stream.streamId}`);

	// Поиск среди зарегистрированных и кнопка «Добавить» в строке.
	await staffPage.getByLabel('Поиск среди зарегистрированных').fill(student.email);
	const found = staffPage.getByRole('list', { name: 'Найденные пользователи' }).getByRole('listitem');
	await expect(found).toHaveCount(1);
	await expect(found).toContainText('email не подтверждён');
	await found.getByRole('button', { name: 'Добавить' }).click();
	await expect(found).toContainText('в потоке');
	await expect(staffPage.getByText(student.email, { exact: true }).last()).toBeVisible();

	// Запасной путь — список email: незнакомый адрес попадает в отчёт.
	await staffPage.getByText('Вставить список email').click();
	await staffPage.getByLabel('Email студентов').fill(`${student.email.toUpperCase()}\n${ghost}`);
	await staffPage.getByRole('button', { name: 'Добавить', exact: true }).last().click();
	const report = staffPage.getByRole('list', { name: 'Итоги добавления' });
	await expect(report.getByRole('listitem').filter({ hasText: student.email })).toContainText('уже в потоке');
	await expect(report.getByRole('listitem').filter({ hasText: ghost })).toContainText('не зарегистрирован(а)');
	await expect(staffPage.getByLabel('Email студентов')).toHaveValue(ghost);

	// Окно не открыто: студент видит тест в «Пока закрыты», начать не может.
	const own = await browser.newContext();
	const ownPage = await own.newPage();
	await signIn(own, ownPage, student);
	const closed = ownPage.getByRole('region', { name: 'Пока закрыты' });
	await expect(closed).toContainText(course.testTitle);
	await expect(closed.getByRole('link', { name: 'Начать' })).toHaveCount(0);
	await ownPage.goto(`/tests/${course.testId}/start`);
	await expect(ownPage.getByText('Тест пока закрыт для вашего потока')).toBeVisible();
	await expect(ownPage.getByRole('button', { name: 'Начать', exact: true })).toHaveCount(0);

	// Ссылка для студентов на странице теста.
	await staffPage.goto(`/tests/${course.testId}`);
	await expect(staffPage.getByTestId('student-link')).toHaveText(new RegExp(`/tests/${course.testId}/start$`));

	// Окно открыто — тест в доступных, по прямой ссылке можно начать.
	await admin.api.call('PUT', `/api/tests/${course.testId}/windows/${stream.streamId}`, { action: 'open' });
	await ownPage.goto(`/tests/${course.testId}/start`);
	await expect(ownPage.getByRole('button', { name: 'Начать', exact: true })).toBeVisible();
	await ownPage.goto('/');
	await expect(ownPage.getByRole('region', { name: 'Доступные сейчас' })).toContainText(course.testTitle);

	await student.api.dispose();
	await own.close();
	await staff.close();
});
