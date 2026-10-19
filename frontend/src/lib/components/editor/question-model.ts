import type { Schema } from '$lib/api/client';
import { normalizeDecimal } from '$lib/format';

export type QuestionKind = Schema<'QuestionKind'>;

export interface ImageDraft {
	id: string;
	url: string;
	alt: string;
}

export interface OptionDraft {
	/** Локальный ключ для `{#each}`. */
	key: string;
	body: string;
	is_correct: boolean;
	/** Ввод пользователя; пусто — штраф по умолчанию. */
	penalty: string;
	pinned: boolean;
	image: ImageDraft | null;
}

export interface QuestionDraft {
	kind: QuestionKind;
	body: string;
	weight: string;
	image: ImageDraft | null;
	options: OptionDraft[];
}

/** Порядок ключей — порядок пунктов в списке «Тип вопроса». */
export const KIND_LABELS: Record<QuestionKind, string> = {
	single: 'Один ответ',
	multiple_all: 'Несколько: только все верные',
	multiple_any: 'Несколько: хотя бы один верный',
	multiple_each: 'Несколько: балл за каждый верный',
	multiple_fraction: 'Несколько: доля верных',
	free_text: 'Свободный ответ'
};

export const KIND_SHORT: Record<QuestionKind, string> = {
	single: 'один верный',
	multiple_all: 'несколько верных, «только все»',
	multiple_any: 'несколько верных, «хотя бы один»',
	multiple_each: 'несколько верных, «балл за каждый»',
	multiple_fraction: 'несколько верных, «доля верных»',
	free_text: 'свободный ответ'
};

export const KIND_HINTS: Record<QuestionKind, string> = {
	single: 'Студент выбирает один вариант. Верный вариант должен быть ровно один.',
	multiple_all:
		'Балл ставится, только если выбраны все верные варианты и ни одного неверного.',
	multiple_any:
		'Полный вес, если выбран хотя бы один верный вариант; за каждый выбранный неверный — штраф (по умолчанию −вес).',
	multiple_each:
		'Вес за каждый выбранный верный вариант; максимум — вес × число верных. Штраф выбранного неверного вычитается как есть.',
	multiple_fraction:
		'Доля веса: вес × выбранные верные ÷ все верные. Штраф выбранного неверного: вес × штраф ÷ число неверных.',
	free_text: 'Студент пишет ответ текстом. Такой вопрос не оценивается автоматически.'
};

/** Как понимается штраф неверного варианта в каждом режиме; пусто — штраф по умолчанию. */
export const PENALTY_HINTS: Record<QuestionKind, string> = {
	single: 'Штраф — число ≤ 0 в баллах за выбор неверного варианта; пусто — 0.',
	multiple_all: 'Штраф — число ≤ 0 в баллах за выбор неверного варианта; пусто — 0.',
	multiple_any: 'Штраф — число ≤ 0 в баллах за выбор неверного варианта; пусто — −вес вопроса.',
	multiple_each: 'Штраф — число ≤ 0 в баллах, вычитается как есть; пусто — 0.',
	multiple_fraction:
		'Штраф — число ≤ 0 в долях веса: снимается вес × штраф ÷ число неверных вариантов; пусто — 0.',
	free_text: ''
};

let counter = 0;
export function newKey(): string {
	counter += 1;
	return `k${counter}-${Math.random().toString(36).slice(2, 8)}`;
}

function imageFrom(ref: Schema<'ImageRef'> | null | undefined): ImageDraft | null {
	return ref ? { id: ref.id, url: ref.url, alt: ref.alt } : null;
}

export function emptyOption(): OptionDraft {
	return { key: newKey(), body: '', is_correct: false, penalty: '', pinned: false, image: null };
}

export function emptyDraft(kind: QuestionKind = 'single'): QuestionDraft {
	return {
		kind,
		body: '',
		weight: '1',
		image: null,
		options: kind === 'free_text' ? [] : [emptyOption(), emptyOption()]
	};
}

export function fromDetail(question: Schema<'QuestionDetail'>): QuestionDraft {
	return {
		kind: question.kind,
		body: question.body,
		weight: question.weight,
		image: imageFrom(question.image),
		options: question.options.map((option) => ({
			key: newKey(),
			body: option.body,
			is_correct: option.is_correct,
			// Штраф по умолчанию сервер считает сам от текущего веса — его не фиксируем.
			penalty: option.penalty_custom ? option.penalty : '',
			pinned: option.pinned,
			image: imageFrom(option.image)
		}))
	};
}

export function isMultiple(kind: QuestionKind): boolean {
	return kind !== 'single' && kind !== 'free_text';
}

/** Балл за полностью верный ответ; `null` — свободный ответ или вес не число. */
export function maxScore(draft: QuestionDraft): number | null {
	const weight = Number(normalizeDecimal(draft.weight));
	if (draft.kind === 'free_text' || !Number.isFinite(weight) || weight <= 0) return null;
	if (draft.kind !== 'multiple_each') return weight;
	const correct = draft.options.filter((option) => option.is_correct).length;
	return Math.round(weight * correct * 100) / 100;
}

/** Ошибки черновика; пустой список — можно сохранять. */
export function validateDraft(draft: QuestionDraft): string[] {
	const errors: string[] = [];
	if (draft.body.trim() === '' && !draft.image) errors.push('Добавьте текст вопроса или картинку.');

	const weight = normalizeDecimal(draft.weight);
	if (weight === null || weight === '' || Number(weight) <= 0) {
		errors.push('Вес — положительное число, например 1 или 0,5.');
	}

	if (draft.kind === 'free_text') return errors;

	const options = draft.options;
	if (draft.kind === 'single' && options.length < 2) errors.push('Нужно минимум два варианта ответа.');
	if (isMultiple(draft.kind) && options.length < 2) errors.push('Нужно минимум два варианта ответа.');

	options.forEach((option, index) => {
		if (option.body.trim() === '' && !option.image) {
			errors.push(`Вариант ${index + 1}: добавьте текст или картинку.`);
		}
		const penalty = normalizeDecimal(option.penalty);
		if (penalty === null) errors.push(`Вариант ${index + 1}: штраф — число, например −0,5.`);
		else if (penalty !== '' && Number(penalty) > 0) {
			errors.push(`Вариант ${index + 1}: штраф не может быть больше нуля.`);
		}
	});

	const correct = options.filter((option) => option.is_correct).length;
	if (draft.kind === 'single' && correct !== 1) errors.push('Отметьте ровно один верный вариант.');
	if (isMultiple(draft.kind) && correct < 1) errors.push('Отметьте хотя бы один верный вариант.');
	return errors;
}

/** Черновик → тело запроса. Вызывать после `validateDraft`. */
export function toInput(draft: QuestionDraft): Schema<'QuestionInput'> {
	return {
		kind: draft.kind,
		body: draft.body,
		weight: normalizeDecimal(draft.weight) || '1',
		image_id: draft.image?.id ?? null,
		image_alt: draft.image?.alt ?? '',
		options:
			draft.kind === 'free_text'
				? []
				: draft.options.map((option) => {
						const penalty = normalizeDecimal(option.penalty);
						return {
							body: option.body,
							is_correct: option.is_correct,
							penalty: penalty === '' || penalty === null ? null : penalty,
							pinned: option.pinned,
							image_id: option.image?.id ?? null,
							image_alt: option.image?.alt ?? ''
						};
					})
	};
}

/** Смена типа вопроса: для одиночного выбора оставляем максимум один верный. */
export function changeKind(draft: QuestionDraft, kind: QuestionKind): QuestionDraft {
	let options = draft.options;
	if (kind !== 'free_text' && options.length === 0) options = [emptyOption(), emptyOption()];
	if (kind === 'single') {
		let seen = false;
		options = options.map((option) => {
			const keep = option.is_correct && !seen;
			if (option.is_correct) seen = true;
			return { ...option, is_correct: keep };
		});
	}
	return { ...draft, kind, options };
}
