import { describe, expect, it } from 'vitest';
import {
	exportUrl,
	eventLabel,
	formatSeconds,
	nextSort,
	sortRows,
	sortSummaryRows,
	type AttemptEvent,
	type ResultsRow,
	type SummaryRow
} from './table';

function row(name: string, score: string, q1: { score: string | null; in_attempt?: boolean; free_text?: boolean }): ResultsRow {
	return {
		attempt_id: `a-${name}`,
		user_id: `u-${name}`,
		display_name: name,
		email: `${name}@example.org`,
		removed: false,
		attempt_number: 1,
		started_at: '2026-10-13T10:00:00Z',
		submitted_at: '2026-10-13T10:30:00Z',
		duration_seconds: 1800,
		score,
		max_score: '2',
		antifraud: false,
		cells: [
			{
				question_id: 'q1',
				in_attempt: q1.in_attempt ?? true,
				free_text: q1.free_text ?? false,
				score: q1.score,
				selected: []
			}
		]
	};
}

describe('exportUrl', () => {
	it('добавляет расширение и только непустые параметры', () => {
		expect(exportUrl('/api/tests/t1/results', 'csv', { stream_id: '', mode: 'final' })).toBe(
			'/api/tests/t1/results.csv?mode=final'
		);
		expect(exportUrl('/api/tests/t1/results', 'xlsx', { stream_id: 's1', mode: 'all' })).toBe(
			'/api/tests/t1/results.xlsx?stream_id=s1&mode=all'
		);
		expect(exportUrl('/x', 'csv', {})).toBe('/x.csv');
	});
});

describe('sortRows', () => {
	const rows = [
		row('Борис', '1', { score: '0.5' }),
		row('Анна', '2', { score: null, in_attempt: false }),
		row('Вера', '0', { score: '1' })
	];

	it('по имени', () => {
		expect(sortRows(rows, 'student', 'asc').map((r) => r.display_name)).toEqual(['Анна', 'Борис', 'Вера']);
	});

	it('по баллу за вопрос: «—» всегда в конце', () => {
		expect(sortRows(rows, 'q:q1', 'desc').map((r) => r.display_name)).toEqual(['Вера', 'Борис', 'Анна']);
		expect(sortRows(rows, 'q:q1', 'asc').map((r) => r.display_name)).toEqual(['Борис', 'Вера', 'Анна']);
	});

	it('по общему баллу', () => {
		expect(sortRows(rows, 'score', 'desc').map((r) => r.display_name)).toEqual(['Анна', 'Борис', 'Вера']);
	});
});

describe('sortSummaryRows', () => {
	const summary: SummaryRow[] = [
		{ user_id: '1', display_name: 'А', email: '', cells: [{ test_id: 't', final_score: '3', max_score: '5', attempts: 1 }] },
		{ user_id: '2', display_name: 'Б', email: '', cells: [{ test_id: 't', final_score: null, max_score: null, attempts: 0 }] },
		{ user_id: '3', display_name: 'В', email: '', cells: [{ test_id: 't', final_score: '5', max_score: '5', attempts: 2 }] }
	];
	it('«нет попыток» — в конце при любом направлении', () => {
		expect(sortSummaryRows(summary, 't:t', 'desc').map((r) => r.display_name)).toEqual(['В', 'А', 'Б']);
		expect(sortSummaryRows(summary, 't:t', 'asc').map((r) => r.display_name)).toEqual(['А', 'В', 'Б']);
	});
});

describe('прочее', () => {
	it('nextSort переключает направление', () => {
		expect(nextSort({ key: 'student', dir: 'asc' }, 'student')).toEqual({ key: 'student', dir: 'desc' });
		expect(nextSort({ key: 'student', dir: 'asc' }, 'score')).toEqual({ key: 'score', dir: 'desc' });
	});

	it('formatSeconds', () => {
		expect(formatSeconds(65)).toBe('1:05');
		expect(formatSeconds(3723)).toBe('1:02:03');
	});

	it('события попытки по-русски', () => {
		// В OpenAPI `details` — пустой объект, поэтому собираем событие с приведением типа.
		const ev = (kind: string, details: Record<string, unknown>, question_id?: string) =>
			({ at: '', kind, details, question_id }) as unknown as AttemptEvent;
		const numberOf = (id: string) => (id === 'q3' ? 3 : undefined);
		expect(eventLabel(ev('navigate', { from: 1, to: 2 }), numberOf)).toBe('Переход В2 → В3');
		expect(
			eventLabel(ev('answer_saved', { selected: ['x'], text_length: 0 }, 'q3'), numberOf)
		).toBe('Сохранён ответ на В3 (вариантов: 1)');
		expect(eventLabel(ev('submit', { reason: 'timeout', score: '1.5' }), numberOf)).toBe(
			'Попытка сдана (сдана по времени), балл 1,5'
		);
		expect(eventLabel(ev('tab_hidden', {}), numberOf)).toBe('Вкладка скрыта');
	});
});
