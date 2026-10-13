import createClient, { type Middleware } from 'openapi-fetch';
import type { components, paths } from './schema';

export type CurrentUser = components['schemas']['CurrentUser'];
export type SessionInfo = components['schemas']['SessionInfo'];
export type MeResponse = components['schemas']['MeResponse'];

/** Тело ошибки бэкенда: `{ error, message }`. */
export interface ErrorBody {
	error: string;
	message: string;
}

/** Ошибка запроса к API с понятным пользователю текстом. */
export class ApiError extends Error {
	readonly status: number;
	readonly code: string;
	readonly retryAfter: number | null;

	constructor(status: number, code: string, message: string, retryAfter: number | null = null) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
		this.code = code;
		this.retryAfter = retryAfter;
	}
}

const UNSAFE_METHODS = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);

/**
 * Бэкенд принимает изменяющие запросы только с `Content-Type: application/json`
 * (CSRF, D4). openapi-fetch ставит его лишь при наличии тела — дополняем.
 */
const jsonContentType: Middleware = {
	onRequest({ request }) {
		if (UNSAFE_METHODS.has(request.method) && !request.headers.has('Content-Type')) {
			request.headers.set('Content-Type', 'application/json');
		}
		return request;
	}
};

/** Типизированный клиент API. Тот же origin, сессия — в HttpOnly cookie. */
export const api = createClient<paths>({ baseUrl: '', credentials: 'same-origin' });
api.use(jsonContentType);

const FALLBACK_MESSAGES: Record<number, string> = {
	400: 'Проверьте введённые данные.',
	401: 'Нужно войти в систему.',
	403: 'Недостаточно прав для этого действия.',
	404: 'Не найдено.',
	409: 'Конфликт данных: обновите страницу и попробуйте ещё раз.',
	410: 'Ссылка устарела.',
	413: 'Слишком большой запрос.',
	429: 'Слишком много попыток. Повторите позже.',
	500: 'Внутренняя ошибка сервера. Попробуйте позже.',
	502: 'Сервер временно недоступен. Попробуйте позже.',
	503: 'Сервер временно недоступен. Попробуйте позже.',
	504: 'Сервер не ответил вовремя. Попробуйте позже.'
};

function isErrorBody(value: unknown): value is ErrorBody {
	return (
		typeof value === 'object' &&
		value !== null &&
		typeof (value as Record<string, unknown>).error === 'string' &&
		typeof (value as Record<string, unknown>).message === 'string'
	);
}

/** Превращает ответ-ошибку в `ApiError` с текстом для пользователя. */
export function toApiError(response: Response, body: unknown): ApiError {
	const retryHeader = response.headers.get('Retry-After');
	const retryAfter = retryHeader !== null && /^\d+$/.test(retryHeader) ? Number(retryHeader) : null;
	if (isErrorBody(body) && body.message.trim() !== '') {
		return new ApiError(response.status, body.error, body.message, retryAfter);
	}
	const message =
		FALLBACK_MESSAGES[response.status] ??
		(response.status >= 500
			? 'Ошибка сервера. Попробуйте позже.'
			: `Не удалось выполнить запрос (код ${response.status}).`);
	return new ApiError(response.status, 'http_' + response.status, message, retryAfter);
}

/** Текст для пользователя из любой ошибки (в т.ч. сетевой). */
export function errorMessage(error: unknown): string {
	if (error instanceof ApiError) return error.message;
	if (error instanceof TypeError) return 'Нет связи с сервером. Проверьте подключение к интернету.';
	return 'Что-то пошло не так. Попробуйте ещё раз.';
}

type FetchResult<T> = { data?: T; error?: unknown; response: Response };

/**
 * Возвращает данные успешного ответа или бросает `ApiError`.
 * Сетевые сбои пробрасываются как есть (TypeError) — `errorMessage` их распознаёт.
 */
export async function unwrap<T>(request: Promise<FetchResult<T>>): Promise<T> {
	const { data, error, response } = await request;
	if (!response.ok) throw toApiError(response, error);
	return data as T;
}
