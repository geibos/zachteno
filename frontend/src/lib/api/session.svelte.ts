import { goto } from '$app/navigation';
import { api, errorMessage, unwrap, type CurrentUser } from './client';

type Status = 'idle' | 'loading' | 'ready' | 'error';

/** Текущий пользователь и настройки стенда (`GET /api/auth/me`). */
class SessionStore {
	user = $state<CurrentUser | null>(null);
	yandexEnabled = $state(false);
	status = $state<Status>('idle');
	error = $state<string | null>(null);
	/** Идёт выход: защищённые страницы не должны уводить на /login. */
	leaving = $state(false);
	/** Назначен ли преподавателем хотя бы одного курса (`teaches` из `/me`). */
	teaches = $state(false);

	#pending: Promise<void> | null = null;

	get ready(): boolean {
		return this.status === 'ready';
	}

	/** Методист, суперпользователь или преподаватель: видит курсы и потоки. */
	get isStaff(): boolean {
		const roles = this.user?.roles;
		return Boolean(roles && (roles.methodist || roles.superuser)) || this.teaches;
	}

	/** Загружает сессию; параллельные вызовы ждут один запрос. */
	load(): Promise<void> {
		this.#pending ??= this.#fetch().finally(() => {
			this.#pending = null;
		});
		return this.#pending;
	}

	async #fetch(): Promise<void> {
		if (this.status !== 'ready') this.status = 'loading';
		try {
			const me = await unwrap(api.GET('/api/auth/me'));
			this.user = me.user ?? null;
			this.yandexEnabled = me.yandex_enabled;
			this.teaches = me.teaches;
			this.error = null;
			this.status = 'ready';
		} catch (error) {
			this.error = errorMessage(error);
			this.status = 'error';
		}
	}

	/**
	 * После входа/регистрации сервер уже вернул пользователя. Признак `teaches`
	 * приходит только в `/me` — догружаем его в фоне, экран не мигает.
	 */
	setUser(user: CurrentUser | null): void {
		this.user = user;
		this.status = 'ready';
		this.teaches = false;
		if (user) void this.load();
	}

	/** Выход: сессия удаляется на сервере, затем переход на главную. */
	async logout(): Promise<void> {
		await unwrap(api.POST('/api/auth/logout'));
		this.leaving = true;
		try {
			await goto('/');
		} finally {
			this.user = null;
			this.teaches = false;
			this.leaving = false;
		}
	}
}

export const session = new SessionStore();
