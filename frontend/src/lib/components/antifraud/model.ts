import type { Schema } from '$lib/api/client';

export type LinkView = Schema<'LinkView'>;
export type EvidenceKind = Schema<'EvidenceKind'>;
export type LinkedUser = Schema<'LinkedUser'>;

export type Level = 'high' | 'medium' | 'low';
export type LevelFilter = 'all' | Level;
export type DecisionFilter = 'all' | 'none' | 'confirmed' | 'rejected';

/** Нативный `<select>` в стиле полей shadcn. */
export const selectClass =
	'border-input bg-background dark:bg-input/30 focus-visible:border-ring focus-visible:ring-ring/50 h-9 w-full min-w-0 rounded-md border px-2.5 text-sm shadow-xs outline-none focus-visible:ring-3 disabled:cursor-not-allowed disabled:opacity-50';

export const EVIDENCE_LABELS: Record<EvidenceKind, string> = {
	shared_marker: 'общий маркер браузера',
	shared_rare_device: 'редкое общее устройство',
	shared_common_device: 'типовое общее устройство',
	home_network: 'домашняя сеть',
	session_handoff: 'передача сессии',
	identical_free_text: 'одинаковые свободные ответы',
	matching_mistakes: 'совпадающие ошибки',
	typing_similarity: 'похожий ритм набора',
	registration_pattern: 'похожая регистрация',
	shared_public_network: 'общая публичная сеть',
	transitive_device: 'связь через общее устройство с третьим аккаунтом',
	same_hardware: 'тот же компьютер в другом браузере (слабый признак)'
};

export function evidenceLabel(kind: string): string {
	return (EVIDENCE_LABELS as Record<string, string>)[kind] ?? kind;
}

export const LEVEL_LABELS: Record<Level, string> = {
	high: 'высокий',
	medium: 'средний',
	low: 'низкий'
};

export function levelLabel(level: string): string {
	return (LEVEL_LABELS as Record<string, string>)[level] ?? level;
}

const LEVEL_RANK: Record<string, number> = { high: 0, medium: 1, low: 2 };

export function decisionLabel(decision: string | null | undefined): string {
	if (decision === 'confirmed') return 'подтверждено';
	if (decision === 'rejected') return 'отклонено';
	return 'без решения';
}

/** Ключ пары для списков. */
export function linkKey(link: LinkView): string {
	return `${link.user_a.id}:${link.user_b.id}`;
}

/** Порядок: high → medium → low; внутри уровня сначала без решения. */
export function sortLinks(links: readonly LinkView[]): LinkView[] {
	return [...links].sort((a, b) => {
		const level = (LEVEL_RANK[a.level] ?? 3) - (LEVEL_RANK[b.level] ?? 3);
		if (level !== 0) return level;
		const decided = Number(Boolean(a.decision)) - Number(Boolean(b.decision));
		if (decided !== 0) return decided;
		return b.computed_at.localeCompare(a.computed_at);
	});
}

export function filterLinks(
	links: readonly LinkView[],
	level: LevelFilter,
	decision: DecisionFilter
): LinkView[] {
	return links.filter((link) => {
		if (level !== 'all' && link.level !== level) return false;
		if (decision === 'none') return !link.decision;
		if (decision !== 'all') return link.decision === decision;
		return true;
	});
}

/** Пользователи, упомянутые в связях, по id — чтобы показать имена в кластерах. */
export function usersById(links: readonly LinkView[]): Map<string, LinkedUser> {
	const users = new Map<string, LinkedUser>();
	for (const link of links) {
		users.set(link.user_a.id, link.user_a);
		users.set(link.user_b.id, link.user_b);
	}
	return users;
}

/** Сокращённый идентификатор: первые 8 символов UUID. */
export function shortId(id: string): string {
	return id.slice(0, 8);
}
