import { describe, expect, it } from 'vitest';
import { changeKind, emptyDraft, toInput, validateDraft, type QuestionDraft } from './question-model';

function draft(patch: Partial<QuestionDraft> = {}): QuestionDraft {
	const base = emptyDraft('single');
	base.body = 'Сколько будет $2+2$?';
	base.options[0]!.body = '4';
	base.options[0]!.is_correct = true;
	base.options[1]!.body = '5';
	return { ...base, ...patch };
}

describe('валидация вопроса', () => {
	it('корректный одиночный выбор', () => {
		expect(validateDraft(draft())).toEqual([]);
	});

	it('одиночный выбор требует ровно один верный', () => {
		const d = draft();
		d.options[1]!.is_correct = true;
		expect(validateDraft(d)).toContain('Отметьте ровно один верный вариант.');
	});

	it('множественный выбор требует хотя бы один верный и два варианта', () => {
		const d = draft({ kind: 'multiple_any' });
		d.options.forEach((o) => (o.is_correct = false));
		expect(validateDraft(d)).toContain('Отметьте хотя бы один верный вариант.');
		d.options = d.options.slice(0, 1);
		expect(validateDraft(d)).toContain('Нужно минимум два варианта ответа.');
	});

	it('вариант может быть только картинкой', () => {
		const d = draft();
		d.options[1]!.body = '';
		d.options[1]!.image = { id: 'img', url: '/api/files/img', alt: 'график' };
		expect(validateDraft(d)).toEqual([]);
		d.options[1]!.image = null;
		expect(validateDraft(d)).toContain('Вариант 2: добавьте текст или картинку.');
	});

	it('штраф не может быть положительным', () => {
		const d = draft();
		d.options[1]!.penalty = '0,5';
		expect(validateDraft(d)).toContain('Вариант 2: штраф не может быть больше нуля.');
	});

	it('свободный ответ без вариантов', () => {
		const d = changeKind(draft(), 'free_text');
		expect(validateDraft(d)).toEqual([]);
		expect(toInput(d).options).toEqual([]);
	});
});

describe('toInput', () => {
	it('десятичные строками, пустой штраф — null, картинки — image_id', () => {
		const d = draft({ weight: '1,5' });
		d.options[1]!.penalty = '-0,25';
		d.image = { id: 'q-img', url: '/api/files/q-img', alt: 'схема' };
		const input = toInput(d);
		expect(input.weight).toBe('1.5');
		expect(input.image_id).toBe('q-img');
		expect(input.image_alt).toBe('схема');
		expect(input.options?.[0]?.penalty).toBeNull();
		expect(input.options?.[1]?.penalty).toBe('-0.25');
	});

	it('смена на одиночный выбор оставляет один верный', () => {
		const d = draft({ kind: 'multiple_all' });
		d.options.forEach((o) => (o.is_correct = true));
		const single = changeKind(d, 'single');
		expect(single.options.filter((o) => o.is_correct)).toHaveLength(1);
	});
});
