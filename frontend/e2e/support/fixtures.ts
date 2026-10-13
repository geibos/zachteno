import { test as base } from '@playwright/test';
import { adminAccount, type Account } from './api';

/**
 * `admin` — суперпользователь-методист стенда; входит один раз на воркер,
 * чтобы не упираться в ограничение частоты входа.
 */
export const test = base.extend<object, { admin: Account }>({
	admin: [
		async ({}, use) => {
			const account = await adminAccount();
			await use(account);
			await account.api.dispose();
		},
		{ scope: 'worker' }
	]
});

export { expect } from '@playwright/test';
