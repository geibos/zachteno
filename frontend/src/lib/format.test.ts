import { describe, expect, it } from 'vitest';
import { formatPercent } from './format';

describe('formatPercent', () => {
	it('один знак, запятая и знак процента', () => {
		expect(formatPercent('66.7')).toBe('66,7 %');
		expect(formatPercent('100')).toBe('100 %');
		expect(formatPercent('-12.5')).toBe('-12,5 %');
	});

	it('нет процента — пустая строка', () => {
		expect(formatPercent(null)).toBe('');
		expect(formatPercent(undefined)).toBe('');
	});
});
