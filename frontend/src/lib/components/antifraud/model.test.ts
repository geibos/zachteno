import { describe, expect, it } from 'vitest';
import { evidenceLabel, filterLinks, levelLabel, sortLinks, usersById, type LinkView } from './model';

function link(a: string, b: string, level: string, decision: string | null = null, computed = '2026-10-01T10:00:00Z'): LinkView {
	return {
		user_a: { id: a, display_name: a.toUpperCase(), email: `${a}@example.org` },
		user_b: { id: b, display_name: b.toUpperCase(), email: `${b}@example.org` },
		level,
		evidence: [],
		decision,
		comment: '',
		computed_at: computed
	};
}

describe('антифрод: связи', () => {
	it('сортирует high → medium → low, без решения первыми', () => {
		const sorted = sortLinks([
			link('a', 'b', 'low'),
			link('c', 'd', 'high', 'confirmed'),
			link('e', 'f', 'medium'),
			link('g', 'h', 'high')
		]);
		expect(sorted.map((l) => l.user_a.id)).toEqual(['g', 'c', 'e', 'a']);
	});

	it('фильтрует по уровню и решению', () => {
		const links = [link('a', 'b', 'high'), link('c', 'd', 'high', 'rejected'), link('e', 'f', 'low', 'confirmed')];
		expect(filterLinks(links, 'high', 'all')).toHaveLength(2);
		expect(filterLinks(links, 'all', 'none').map((l) => l.user_a.id)).toEqual(['a']);
		expect(filterLinks(links, 'all', 'confirmed').map((l) => l.user_a.id)).toEqual(['e']);
		expect(filterLinks(links, 'low', 'rejected')).toHaveLength(0);
	});

	it('подписи и справочник пользователей', () => {
		expect(evidenceLabel('home_network')).toBe('домашняя сеть');
		expect(evidenceLabel('unknown_kind')).toBe('unknown_kind');
		expect(levelLabel('medium')).toBe('средний');
		const users = usersById([link('a', 'b', 'low'), link('b', 'c', 'low')]);
		expect([...users.keys()].sort()).toEqual(['a', 'b', 'c']);
	});
});
