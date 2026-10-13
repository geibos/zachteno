import { test, expect } from './support/fixtures';
import { Api, SESSION_COOKIE, newPassword, registerUser, uniqueEmail, type CurrentUser } from './support/api';
import { loginViaForm, signIn } from './support/browser';
import { MAIL_DIR, NO_MAIL_REASON, waitForLink } from './support/mail';

// Экраны входа (задача 3.12). Суперпользователь не нужен: всё делает посетитель.

/** `GET /api/auth/me`: без сессии `user` — `null`. */
interface Me {
	user: (CurrentUser & { email_verified: boolean }) | null;
}

test('11. регистрация через форму, выход и вход паролем', async ({ page, context }) => {
	const email = uniqueEmail('register');
	const password = newPassword();

	await page.goto('/register');
	await page.locator('#email').fill(email);
	await page.locator('#name').fill('Студент Регистрация');
	await page.locator('#password').fill(password);
	await page.locator('#repeat').fill(password);
	await page.locator('form').getByRole('button', { name: 'Зарегистрироваться', exact: true }).click();
	await page.waitForURL((url) => url.pathname === '/');
	const menu = page.getByRole('button', { name: 'Меню пользователя' });
	await expect(menu).toBeVisible();
	const session = (await context.cookies()).find((cookie) => cookie.name === SESSION_COOKIE)?.value;
	expect(session).toBeTruthy();

	// Выход: сессия удаляется на сервере, старая cookie больше не авторизует.
	await menu.click();
	await page.getByRole('menuitem', { name: 'Выйти' }).click();
	await expect(page.getByText('Вы вышли из системы')).toBeVisible();
	await expect(menu).toHaveCount(0);
	const stale = await Api.create();
	stale.session = session!;
	expect((await stale.call<Me>('GET', '/api/auth/me')).user).toBeNull();
	await stale.dispose();

	// Вход через форму пароля.
	await loginViaForm(page, email, password);
	await page.goto('/profile');
	await expect(page.locator('main')).toContainText(email);
});

test('12. подтверждение email по ссылке из письма; повторно ссылка не действует', async ({ page, context }) => {
	test.skip(!MAIL_DIR, NO_MAIL_REASON);
	const user = await registerUser('verify', 'Студент Подтверждает');
	await signIn(context, page, user);
	await page.goto('/profile');
	await expect(page.locator('main').getByText('не подтверждён', { exact: true })).toBeVisible();

	const link = await waitForLink(user.email, '/verify-email');
	await page.goto(link);
	await expect(page.getByText('Email подтверждён')).toBeVisible();
	const me = await user.api.call<Me>('GET', '/api/auth/me');
	expect(me.user?.email_verified).toBe(true);
	await page.goto('/profile');
	await expect(page.locator('main').getByText('подтверждён', { exact: true })).toBeVisible();

	// Ссылка одноразовая.
	await page.goto(link);
	await expect(page.getByText('Ссылка недействительна', { exact: true })).toBeVisible();
	await user.api.dispose();
});

test('13. восстановление пароля: письмо, новый пароль, другие сессии завершены', async ({ page }) => {
	test.skip(!MAIL_DIR, NO_MAIL_REASON);
	// Сессия `user.api` — «другое устройство», которое должно разлогиниться.
	const user = await registerUser('reset', 'Студент Забыл Пароль');

	await page.goto('/login');
	await page.getByRole('link', { name: 'Забыли пароль?' }).click();
	await page.waitForURL(/\/forgot-password$/);
	await page.locator('#email').fill(user.email);
	await page.getByRole('button', { name: 'Отправить ссылку' }).click();
	await expect(page.getByText('Проверьте почту')).toBeVisible();

	const link = await waitForLink(user.email, '/reset-password');
	await page.goto(link);
	const fresh = newPassword();
	await page.locator('#password').fill(fresh);
	await page.locator('#repeat').fill(fresh);
	await page.getByRole('button', { name: 'Сохранить пароль' }).click();
	await expect(page.getByText('Пароль изменён')).toBeVisible();

	expect((await user.api.call<Me>('GET', '/api/auth/me')).user).toBeNull();
	const oldLogin = await Api.create();
	expect((await oldLogin.raw('POST', '/api/auth/login', { email: user.email, password: user.password })).status()).toBe(401);
	await oldLogin.dispose();
	await loginViaForm(page, user.email, fresh);

	// Ссылка восстановления одноразовая.
	const again = await Api.create();
	expect((await again.raw('POST', '/api/auth/password/reset', { token: link.split('token=')[1], password: newPassword() })).status()).toBeGreaterThanOrEqual(400);
	await again.dispose();
	await user.api.dispose();
});
