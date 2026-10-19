import { readFile } from 'node:fs/promises';
import { test, expect } from './support/fixtures';
import { HAS_ADMIN, NO_ADMIN_REASON, addStream, enrolledStudent, passViaApi, seedTest, unique } from './support/api';
import { signIn } from './support/browser';

test.skip(!HAS_ADMIN, NO_ADMIN_REASON);

test('19. таблица результатов: фильтр по потоку, сортировка по баллу и CSV с тем же фильтром', async ({ page, context, admin }) => {
	const seeded = await seedTest(admin.api, { freeNavigation: true });
	const other = await addStream(admin.api, seeded.courseId, seeded.testId, { name: unique('Поток Второй') });

	// Поток 1: 2 / 2 и 0 / 2; поток 2: 1 / 2.
	const best = await enrolledStudent(seeded, unique('Студент Отличник'));
	const worst = await enrolledStudent(seeded, unique('Студент Без Ответов'));
	const middle = await enrolledStudent(other, unique('Студент Середина'));
	await passViaApi(best.api, seeded.testId, 2);
	await passViaApi(worst.api, seeded.testId, 0);
	await passViaApi(middle.api, seeded.testId, 1);

	await signIn(context, page, admin);
	await page.goto(`/tests/${seeded.testId}/results`);
	const rows = page.locator('table tbody tr');
	const names = async () => (await rows.locator('th[scope=row] .truncate.font-medium').allInnerTexts()).map((name) => name.trim());
	await expect(rows).toHaveCount(3);

	// Сортировка: первый щелчок по «Балл» — по убыванию, второй — по возрастанию.
	const scoreHeader = page.locator('th', { has: page.getByRole('button', { name: 'Балл' }) });
	await scoreHeader.getByRole('button').click();
	await expect(scoreHeader).toHaveAttribute('aria-sort', 'descending');
	expect(await names()).toEqual([best.user.display_name, middle.user.display_name, worst.user.display_name]);
	await scoreHeader.getByRole('button').click();
	await expect(scoreHeader).toHaveAttribute('aria-sort', 'ascending');
	expect(await names()).toEqual([worst.user.display_name, middle.user.display_name, best.user.display_name]);

	// Фильтр по потоку
	await page.locator('#report-stream').selectOption({ label: other.streamName });
	await expect(rows).toHaveCount(1);
	await expect(rows.first()).toContainText(middle.user.display_name);

	// CSV скачивается с тем же фильтром.
	const csvLink = page.getByRole('link', { name: /Скачать CSV/ });
	await expect(csvLink).toHaveAttribute('href', new RegExp(`stream_id=${other.streamId}`));
	const downloadPromise = page.waitForEvent('download');
	await csvLink.click();
	const download = await downloadPromise;
	expect(download.suggestedFilename()).toMatch(/^results.*\.csv$/);
	const body = await readFile((await download.path())!);
	expect([...body.subarray(0, 3)]).toEqual([0xef, 0xbb, 0xbf]);
	const lines = body.toString('utf8').slice(1).split(/\r?\n/).filter(Boolean);
	expect(lines[0]).toContain(';');
	expect(lines).toHaveLength(2); // заголовок и одна строка
	expect(lines[1]).toContain(middle.email);
	expect(lines[1]).toContain(other.streamName);
	for (const account of [best, worst]) expect(body.toString('utf8')).not.toContain(account.email);

	// Excel — с тем же фильтром, настоящий XLSX (zip-архив).
	const xlsxLink = page.getByRole('link', { name: /Скачать Excel/ });
	await expect(xlsxLink).toHaveAttribute('href', new RegExp(`results\\.xlsx\\?stream_id=${other.streamId}`));
	const xlsxPromise = page.waitForEvent('download');
	await xlsxLink.click();
	const xlsx = await xlsxPromise;
	expect(xlsx.suggestedFilename()).toMatch(/^results.*\.xlsx$/);
	const book = await readFile((await xlsx.path())!);
	expect(book.subarray(0, 2).toString('latin1')).toBe('PK');

	for (const account of [best, worst, middle]) await account.api.dispose();
});
