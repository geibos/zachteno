import { describe, expect, it } from 'vitest';
import { consumeJoin, JOIN_FLAG, rememberJoin } from './invite-intent';

function memory() {
	const data = new Map<string, string>();
	return {
		data,
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
		removeItem: (key: string) => void data.delete(key)
	};
}

describe('намерение вступить по приглашению', () => {
	it('без флага (зашли уже вошедшим, с любыми параметрами) — автовступления нет', () => {
		const storage = memory();
		expect(consumeJoin('tok', storage)).toBe(false);
	});

	it('флаг от этой страницы — вступаем один раз и сразу удаляем флаг', () => {
		const storage = memory();
		rememberJoin('tok', storage);
		expect(storage.data.get(JOIN_FLAG)).toBe('tok');
		expect(consumeJoin('tok', storage)).toBe(true);
		expect(storage.data.has(JOIN_FLAG)).toBe(false);
		expect(consumeJoin('tok', storage)).toBe(false);
	});

	it('флаг от другого приглашения не срабатывает и тоже удаляется', () => {
		const storage = memory();
		rememberJoin('other', storage);
		expect(consumeJoin('tok', storage)).toBe(false);
		expect(storage.data.has(JOIN_FLAG)).toBe(false);
	});
});
