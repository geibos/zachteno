import { describe, expect, it } from 'vitest';
import { parseEmailList } from './email-list';

describe('parseEmailList', () => {
	it('разбирает столбец, запятые и точки с запятой', () => {
		expect(parseEmailList('a@x.ru\nb@x.ru, c@x.ru;d@x.ru\t e@x.ru')).toEqual([
			'a@x.ru',
			'b@x.ru',
			'c@x.ru',
			'd@x.ru',
			'e@x.ru'
		]);
	});

	it('убирает повторы без учёта регистра и пустые строки', () => {
		expect(parseEmailList('\n A@x.ru \n\na@X.ru\n,,b@x.ru\n')).toEqual(['A@x.ru', 'b@x.ru']);
	});

	it('пустой текст — пустой список', () => {
		expect(parseEmailList('  \n ')).toEqual([]);
	});
});
