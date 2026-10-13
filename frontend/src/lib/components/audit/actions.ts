/** Подписи действий аудит-лога (коды — из бэкенда). */
export const AUDIT_ACTIONS: ReadonlyArray<readonly [string, string]> = [
	['auth.login', 'Вход'],
	['auth.login_failed', 'Неудачный вход'],
	['auth.password_change', 'Смена пароля'],
	['auth.password_reset', 'Сброс пароля'],
	['auth.identity_unlink', 'Отвязка внешнего аккаунта'],
	['user.register', 'Регистрация'],
	['user.block', 'Блокировка пользователя'],
	['user.unblock', 'Разблокировка пользователя'],
	['role.grant', 'Выдача роли'],
	['role.revoke', 'Снятие роли'],
	['course.create', 'Создание курса'],
	['course.update', 'Изменение курса'],
	['course.archive', 'Архивирование курса'],
	['course.clone', 'Копирование курса'],
	['course.teacher_assign', 'Назначение преподавателя'],
	['course.teacher_remove', 'Снятие преподавателя'],
	['test.create', 'Создание теста'],
	['test.update', 'Изменение теста'],
	['test.publish', 'Публикация теста'],
	['test.archive', 'Архивирование теста'],
	['test.clone', 'Копирование теста'],
	['question.create', 'Создание вопроса'],
	['question.update', 'Изменение вопроса'],
	['question.delete', 'Удаление вопроса'],
	['stream.create', 'Создание потока'],
	['stream.assign_course', 'Назначение курса потоку'],
	['stream.unassign_course', 'Снятие курса с потока'],
	['stream.remove_member', 'Исключение из потока'],
	['invite.create', 'Создание пригласительной ссылки'],
	['window.open', 'Открытие окна'],
	['window.close', 'Закрытие окна'],
	['window.schedule', 'Расписание окна'],
	['antifraud.view', 'Просмотр связей антифрода'],
	['antifraud.decision', 'Решение по связи антифрода'],
	['export.csv', 'Выгрузка CSV']
];

const LABELS = new Map(AUDIT_ACTIONS);

export function actionLabel(action: string): string {
	return LABELS.get(action) ?? action;
}

const OBJECT_TYPES: Record<string, string> = {
	user: 'пользователь',
	course: 'курс',
	test: 'тест',
	question: 'вопрос',
	stream: 'поток',
	invite: 'ссылка',
	window: 'окно',
	attempt: 'попытка',
	session: 'сессия',
	identity: 'внешний аккаунт',
	trusted_link: 'доверенная ссылка'
};

export function objectTypeLabel(type: string): string {
	return OBJECT_TYPES[type] ?? type;
}

/** details → читаемый JSON; пустой объект — `null`. */
export function detailsText(details: unknown): string | null {
	if (details === null || details === undefined) return null;
	if (typeof details === 'object' && Object.keys(details as object).length === 0) return null;
	return JSON.stringify(details, null, 2);
}
