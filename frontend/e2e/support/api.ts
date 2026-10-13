/**
 * Публичный API стенда для подготовки данных в сквозных тестах.
 *
 * Сессия — cookie `__Host-session` с флагом Secure. Чтобы не зависеть от того,
 * отправит ли cookie-jar Playwright Secure-cookie по http://localhost, клиент сам
 * забирает значение из `Set-Cookie` и шлёт заголовок `Cookie`. Изменяющие запросы
 * идут с `Origin` стенда и `Content-Type: application/json` (CSRF-защита бэкенда).
 */
import { request as playwrightRequest, type APIRequestContext, type APIResponse } from '@playwright/test';
import { readFileSync } from 'node:fs';

export const BASE_URL = process.env.E2E_BASE_URL ?? 'http://localhost:8080';
export const ORIGIN = new URL(BASE_URL).origin;
export const SESSION_COOKIE = '__Host-session';

export const ADMIN_EMAIL = process.env.E2E_ADMIN_EMAIL ?? '';
export const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD ?? '';
export const HAS_ADMIN = ADMIN_EMAIL !== '' && ADMIN_PASSWORD !== '';
export const NO_ADMIN_REASON = 'Нужны E2E_ADMIN_EMAIL и E2E_ADMIN_PASSWORD (суперпользователь стенда).';

type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

const RUN = Date.now().toString(36);
let counter = 0;

/** Уникальный суффикс: тесты можно гонять повторно на одном стенде. */
export function unique(prefix: string): string {
	counter += 1;
	return `${prefix} ${RUN}-${counter}`;
}

export function uniqueEmail(tag: string): string {
	counter += 1;
	return `e2e-${tag}-${RUN}-${counter}@example.org`;
}

/** Длинный пароль без требований к составу (не из списка распространённых). */
export function newPassword(): string {
	return `сквозной тест ${RUN} ${Math.random().toString(36).slice(2, 10)}`;
}

export class ApiError extends Error {
	constructor(
		readonly status: number,
		readonly body: string,
		message: string
	) {
		super(message);
	}
}

export class Api {
	session: string | null = null;

	private constructor(readonly ctx: APIRequestContext) {}

	static async create(): Promise<Api> {
		return new Api(await playwrightRequest.newContext({ baseURL: BASE_URL }));
	}

	private headers(method: Method): Record<string, string> {
		const headers: Record<string, string> = { Origin: ORIGIN };
		if (method !== 'GET') headers['Content-Type'] = 'application/json';
		if (this.session) headers.Cookie = `${SESSION_COOKIE}=${this.session}`;
		return headers;
	}

	private capture(response: APIResponse): void {
		for (const header of response.headersArray()) {
			if (header.name.toLowerCase() !== 'set-cookie') continue;
			const match = new RegExp(`^${SESSION_COOKIE}=([^;]*)`).exec(header.value);
			if (match) this.session = match[1] ? match[1] : null;
		}
	}

	/** Запрос без проверки статуса. */
	async raw(method: Method, path: string, body?: unknown): Promise<APIResponse> {
		const response = await this.ctx.fetch(path, {
			method,
			headers: this.headers(method),
			data: body === undefined ? undefined : JSON.stringify(body)
		});
		this.capture(response);
		return response;
	}

	/** Запрос, ожидающий 2xx; тело — JSON или `undefined`. */
	async call<T = unknown>(method: Method, path: string, body?: unknown): Promise<T> {
		const response = await this.raw(method, path, body);
		const text = await response.text();
		if (!response.ok()) {
			throw new ApiError(response.status(), text, `${method} ${path} → ${response.status()}: ${text}`);
		}
		return (text ? JSON.parse(text) : undefined) as T;
	}

	/** Загрузка картинки: multipart, поле `file`. */
	async upload(path: string, filePath: string): Promise<{ id: string; url: string }> {
		const headers: Record<string, string> = { Origin: ORIGIN };
		if (this.session) headers.Cookie = `${SESSION_COOKIE}=${this.session}`;
		const response = await this.ctx.post(path, {
			headers,
			multipart: { file: { name: 'figure.png', mimeType: 'image/png', buffer: readFileSync(filePath) } }
		});
		if (!response.ok()) throw new ApiError(response.status(), await response.text(), `upload → ${response.status()}`);
		return (await response.json()) as { id: string; url: string };
	}

	async dispose(): Promise<void> {
		await this.ctx.dispose();
	}
}

// --- типы ответов, которые нужны тестам ---

export interface CurrentUser {
	id: string;
	email: string;
	display_name: string;
	roles: { superuser: boolean; methodist: boolean };
}

export interface Account {
	api: Api;
	user: CurrentUser;
	email: string;
	password: string;
}

/** Вход суперпользователя из окружения; при необходимости выдаёт себе роль методиста. */
export async function adminAccount(): Promise<Account> {
	const api = await Api.create();
	const user = await api.call<CurrentUser>('POST', '/api/auth/login', {
		email: ADMIN_EMAIL,
		password: ADMIN_PASSWORD
	});
	if (!user.roles.superuser) throw new Error('E2E_ADMIN_EMAIL должен принадлежать суперпользователю');
	if (!user.roles.methodist) {
		await api.call('POST', `/api/admin/users/${user.id}/roles`, { role: 'methodist', granted: true });
	}
	return { api, user, email: ADMIN_EMAIL, password: ADMIN_PASSWORD };
}

/** Новый пользователь через публичную регистрацию. */
export async function registerUser(tag: string, displayName: string): Promise<Account> {
	const api = await Api.create();
	const email = uniqueEmail(tag);
	const password = newPassword();
	const user = await api.call<CurrentUser>('POST', '/api/auth/register', {
		email,
		display_name: displayName,
		password
	});
	return { api, user, email, password };
}

// --- подготовка теста ---

export interface SeedOptions {
	questions?: number;
	freeNavigation?: boolean;
	timeLimitMinutes?: number | null;
	attemptsLimit?: number | null;
	withImage?: string;
}

export interface Seeded {
	courseId: string;
	courseTitle: string;
	testId: string;
	testTitle: string;
	streamId: string;
	inviteToken: string;
}

interface TestSettings {
	attempts_limit?: number | null;
	time_limit_minutes?: number | null;
	free_navigation: boolean;
	[key: string]: unknown;
}

/**
 * Опубликованный тест с вопросами одиночного выбора «Верно» / «Неверно»,
 * поток с назначенным курсом, ссылка-приглашение и открытое окно.
 */
export async function seedTest(admin: Api, options: SeedOptions = {}): Promise<Seeded> {
	const courseTitle = unique('E2E курс');
	const testTitle = unique('E2E тест');
	const { id: courseId } = await admin.call<{ id: string }>('POST', '/api/courses', {
		title: courseTitle,
		description: 'Создан сквозным тестом'
	});
	const { id: testId } = await admin.call<{ id: string }>('POST', `/api/courses/${courseId}/tests`, {
		title: testTitle
	});
	const count = options.questions ?? 2;
	for (let index = 0; index < count; index += 1) {
		const image = index === 0 && options.withImage ? await admin.upload('/api/attachments', options.withImage) : null;
		await admin.call('POST', `/api/tests/${testId}/questions`, {
			kind: 'single',
			body: `Вопрос ${index + 1}: выберите «Верно»`,
			image_id: image?.id ?? null,
			image_alt: image ? 'Рисунок' : '',
			options: [
				{ body: 'Верно', is_correct: true },
				{ body: 'Неверно', is_correct: false }
			]
		});
	}
	const detail = await admin.call<{ settings: TestSettings }>('GET', `/api/tests/${testId}`);
	await admin.call('PATCH', `/api/tests/${testId}`, {
		settings: {
			...detail.settings,
			free_navigation: options.freeNavigation ?? false,
			time_limit_minutes: options.timeLimitMinutes ?? null,
			attempts_limit: options.attemptsLimit === undefined ? 1 : options.attemptsLimit
		}
	});
	await admin.call('POST', `/api/tests/${testId}/publish`);

	const { id: streamId } = await admin.call<{ id: string }>('POST', '/api/streams', { name: unique('E2E поток') });
	await admin.call('POST', `/api/streams/${streamId}/courses`, { course_id: courseId });
	const invite = await admin.call<{ url: string }>('POST', `/api/streams/${streamId}/invites`, { label: 'e2e' });
	await admin.call('PUT', `/api/tests/${testId}/windows/${streamId}`, { action: 'open' });
	return { courseId, courseTitle, testId, testTitle, streamId, inviteToken: tokenFromUrl(invite.url) };
}

export function tokenFromUrl(url: string): string {
	const token = new URL(url, BASE_URL).pathname.split('/').filter(Boolean).pop();
	if (!token) throw new Error(`В ссылке нет токена: ${url}`);
	return token;
}

/** Студент, вступивший в поток по приглашению. */
export async function enrolledStudent(seeded: Seeded, name = 'Студент E2E'): Promise<Account> {
	const student = await registerUser('student', name);
	await student.api.call('POST', `/api/invites/${seeded.inviteToken}/join`);
	return student;
}
