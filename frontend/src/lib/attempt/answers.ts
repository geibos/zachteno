import type { Schema } from '$lib/api/client';
import type { AnswerPayload } from './save-queue';

/** Ответ в интерфейсе раннера. */
export interface LocalAnswer {
	selected: string[];
	text: string;
}

export const EMPTY_ANSWER: LocalAnswer = { selected: [], text: '' };

/** Есть ли ответ: выбран вариант или введён непустой текст. */
export function isAnswered(choice: Schema<'Choice'>, answer: LocalAnswer | undefined): boolean {
	if (!answer) return false;
	return choice === 'free_text' ? answer.text.trim() !== '' : answer.selected.length > 0;
}

/** Ответ интерфейса → тело PUT …/answers/{question_id} (без client_seq). */
export function toPayload(choice: Schema<'Choice'>, answer: LocalAnswer): AnswerPayload {
	return choice === 'free_text'
		? { selected_option_ids: [], text: answer.text }
		: { selected_option_ids: [...answer.selected], text: null };
}

/** Сохранённые на сервере ответы → состояние интерфейса. */
export function fromSaved(saved: Schema<'SavedAnswer'>[]): Record<string, LocalAnswer> {
	const result: Record<string, LocalAnswer> = {};
	for (const answer of saved) {
		result[answer.question_id] = {
			selected: [...answer.selected_option_ids],
			text: answer.text_answer ?? ''
		};
	}
	return result;
}
