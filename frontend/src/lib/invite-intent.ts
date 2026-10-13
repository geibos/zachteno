/**
 * Намерение вступить в поток по приглашению.
 *
 * Автоматический join допустим только как продолжение действия, начатого на
 * странице приглашения: пользователь нажал там «Войти» или «Яндекс». Перед уходом
 * кладём флаг `zt_join=<token>` в sessionStorage; по возвращении join вызывается,
 * только если флаг совпадает с токеном страницы, и флаг сразу удаляется.
 * Параметры URL на это не влияют — иначе любой мог бы вступить вошедшего
 * пользователя в поток, просто дав ему ссылку (CSRF).
 */

export const JOIN_FLAG = 'zt_join';

type FlagStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

function defaultStorage(): FlagStorage | null {
	try {
		return typeof sessionStorage === 'undefined' ? null : sessionStorage;
	} catch {
		return null;
	}
}

/** Запомнить, что пользователь сам начал вход/регистрацию ради этого приглашения. */
export function rememberJoin(token: string, storage: FlagStorage | null = defaultStorage()): void {
	try {
		storage?.setItem(JOIN_FLAG, token);
	} catch {
		// без sessionStorage после входа понадобится нажать «Вступить» вручную
	}
}

/** Забирает флаг (удаляя его) и сообщает, относится ли он к этому приглашению. */
export function consumeJoin(token: string, storage: FlagStorage | null = defaultStorage()): boolean {
	try {
		const flag = storage?.getItem(JOIN_FLAG) ?? null;
		storage?.removeItem(JOIN_FLAG);
		return flag !== null && token !== '' && flag === token;
	} catch {
		return false;
	}
}
