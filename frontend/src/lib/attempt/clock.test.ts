import { describe, expect, it } from 'vitest';
import { clockOffset, remainingMs } from './clock';
import { formatDuration, isoToMskInput, mskInputToIso, normalizeDecimal, plural } from '$lib/format';

describe('таймер попытки', () => {
	it('считает остаток по серверному времени, а не по часам устройства', () => {
		// Часы устройства спешат на 10 минут.
		const serverNow = '2026-10-13T10:00:00Z';
		const clientNow = Date.parse('2026-10-13T10:10:00Z');
		const offset = clockOffset(serverNow, clientNow);
		expect(offset).toBe(-10 * 60_000);
		const deadline = '2026-10-13T10:30:00Z';
		expect(remainingMs(deadline, offset, clientNow)).toBe(30 * 60_000);
		// Через 5 минут по часам устройства.
		expect(remainingMs(deadline, offset, clientNow + 5 * 60_000)).toBe(25 * 60_000);
	});

	it('не уходит в минус и возвращает null без лимита', () => {
		expect(remainingMs('2026-10-13T10:00:00Z', 0, Date.parse('2026-10-13T11:00:00Z'))).toBe(0);
		expect(remainingMs(null, 0, Date.now())).toBeNull();
	});

	it('форматирует остаток', () => {
		expect(formatDuration(65_000)).toBe('01:05');
		expect(formatDuration(3_725_000)).toBe('1:02:05');
		expect(formatDuration(-5)).toBe('00:00');
		// Неполная секунда округляется вверх: «00:00» только когда время вышло.
		expect(formatDuration(200)).toBe('00:01');
	});
});

describe('время МСК', () => {
	it('UTC → поле ввода по МСК и обратно', () => {
		expect(isoToMskInput('2026-10-13T21:30:00Z')).toBe('2026-10-14T00:30');
		expect(mskInputToIso('2026-10-14T00:30')).toBe('2026-10-13T21:30:00.000Z');
		expect(mskInputToIso('')).toBeNull();
		expect(isoToMskInput(null)).toBe('');
	});
});

describe('числа', () => {
	it('десятичные с запятой', () => {
		expect(normalizeDecimal('1,5')).toBe('1.5');
		expect(normalizeDecimal('-0.25')).toBe('-0.25');
		expect(normalizeDecimal('')).toBe('');
		expect(normalizeDecimal('abc')).toBeNull();
		expect(normalizeDecimal('1.234')).toBeNull();
	});

	it('склонения', () => {
		expect(plural(1, ['вопрос', 'вопроса', 'вопросов'])).toBe('вопрос');
		expect(plural(3, ['вопрос', 'вопроса', 'вопросов'])).toBe('вопроса');
		expect(plural(11, ['вопрос', 'вопроса', 'вопросов'])).toBe('вопросов');
		expect(plural(22, ['вопрос', 'вопроса', 'вопросов'])).toBe('вопроса');
	});
});
