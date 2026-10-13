import type { Schema } from '$lib/api/client';
import { submitReasonText } from '$lib/attempt/review-status';

export type Mode = 'final' | 'all';
export type ResultsRow = Schema<'TableRow'>;
export type ResultsCell = Schema<'Cell'>;
export type SortDir = 'asc' | 'desc';
/** `student`, `score`, `submitted`, `duration` или `q:<question_id>` / `t:<test_id>`. */
export type SortKey = string;

/** Фильтры отчёта → строка запроса; пустые значения не попадают в URL. */
export function reportQuery(params: { stream_id?: string | null; mode?: Mode | null }): string {
	const search = new URLSearchParams();
	if (params.stream_id) search.set('stream_id', params.stream_id);
	if (params.mode) search.set('mode', params.mode);
	const query = search.toString();
	return query ? `?${query}` : '';
}

/** Ссылка на CSV с теми же фильтрами, что и таблица. */
export function csvUrl(base: string, params: { stream_id?: string | null; mode?: Mode | null }): string {
	return base + reportQuery(params);
}

/** Сравнение с «пустыми» значениями всегда в конце, независимо от направления. */
export function compareNullable(
	a: number | string | null,
	b: number | string | null,
	dir: SortDir
): number {
	const aEmpty = a === null || (typeof a === 'number' && Number.isNaN(a));
	const bEmpty = b === null || (typeof b === 'number' && Number.isNaN(b));
	if (aEmpty && bEmpty) return 0;
	if (aEmpty) return 1;
	if (bEmpty) return -1;
	const result =
		typeof a === 'string' && typeof b === 'string'
			? a.localeCompare(b, 'ru')
			: Number(a) - Number(b);
	return dir === 'asc' ? result : -result;
}

function toNumber(value: string | null | undefined): number | null {
	if (value === null || value === undefined || value === '') return null;
	const number = Number(value);
	return Number.isNaN(number) ? null : number;
}

/** Значение ячейки вопроса для сортировки: «—» и свободный ответ — пустые. */
export function cellSortValue(cell: ResultsCell | undefined): number | null {
	if (!cell || !cell.in_attempt || cell.free_text) return null;
	return toNumber(cell.score);
}

function rowValue(row: ResultsRow, key: SortKey): number | string | null {
	switch (key) {
		case 'student':
			return row.display_name;
		case 'score':
			return toNumber(row.score);
		case 'submitted':
			return Date.parse(row.submitted_at);
		case 'duration':
			return row.duration_seconds;
		default: {
			const questionId = key.startsWith('q:') ? key.slice(2) : key;
			return cellSortValue(row.cells.find((cell) => cell.question_id === questionId));
		}
	}
}

/** Сортировка строк результатов; стабильна — при равенстве порядок исходный. */
export function sortRows(rows: ResultsRow[], key: SortKey, dir: SortDir): ResultsRow[] {
	return rows
		.map((row, index) => ({ row, index }))
		.sort((a, b) => compareNullable(rowValue(a.row, key), rowValue(b.row, key), dir) || a.index - b.index)
		.map(({ row }) => row);
}

/** Длительность попытки: «4:05» или «1:02:03». */
export function formatSeconds(total: number): string {
	const seconds = Math.max(0, Math.round(total));
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = seconds % 60;
	const pad = (value: number) => String(value).padStart(2, '0');
	return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

const mskWithSeconds = new Intl.DateTimeFormat('ru-RU', {
	timeZone: 'Europe/Moscow',
	day: 'numeric',
	month: 'short',
	hour: '2-digit',
	minute: '2-digit',
	second: '2-digit'
});

/** Время события по МСК с секундами. */
export function formatMskSeconds(iso: string): string {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return iso;
	return `${mskWithSeconds.format(date)} МСК`;
}

export type AttemptEvent = Schema<'AttemptEvent'>;

function detail(event: AttemptEvent, name: string): unknown {
	const details = event.details as Record<string, unknown> | null | undefined;
	return details && typeof details === 'object' ? details[name] : undefined;
}

/** Событие попытки → подпись по-русски. `numberOf` — номер вопроса по его id. */
export function eventLabel(event: AttemptEvent, numberOf: (questionId: string) => number | undefined): string {
	const question = event.question_id ? numberOf(event.question_id) : undefined;
	const q = question !== undefined ? `В${question}` : 'вопрос';
	switch (event.kind) {
		case 'start': {
			const questions = detail(event, 'questions');
			return typeof questions === 'number' ? `Начало попытки (${questions} вопр.)` : 'Начало попытки';
		}
		case 'answer_saved': {
			const selected = detail(event, 'selected');
			const textLength = detail(event, 'text_length');
			const parts: string[] = [];
			if (Array.isArray(selected) && selected.length > 0) parts.push(`вариантов: ${selected.length}`);
			if (typeof textLength === 'number' && textLength > 0) parts.push(`текст: ${textLength} симв.`);
			if (parts.length === 0) parts.push('ответ очищен');
			return `Сохранён ответ на ${q} (${parts.join(', ')})`;
		}
		case 'navigate': {
			const from = detail(event, 'from');
			const to = detail(event, 'to');
			if (typeof from === 'number' && typeof to === 'number') return `Переход В${from + 1} → В${to + 1}`;
			return 'Переход к следующему вопросу';
		}
		case 'tab_hidden':
			return 'Вкладка скрыта';
		case 'tab_visible':
			return 'Вкладка снова открыта';
		case 'offline':
			return 'Пропала связь';
		case 'online':
			return 'Связь восстановлена';
		case 'submit': {
			const reason = submitReasonText(String(detail(event, 'reason') ?? ''));
			const score = detail(event, 'score');
			const tail = typeof score === 'string' ? `, балл ${score.replace('.', ',')}` : '';
			return `Попытка сдана${reason ? ` (${reason})` : ''}${tail}`;
		}
		default:
			return event.kind;
	}
}

/** События, которые стоит подсветить при разборе подозрительной попытки. */
export function isNotableEvent(kind: string): boolean {
	return kind === 'tab_hidden' || kind === 'offline';
}

// --- сводная таблица курса ---

/** Ответ GET /api/courses/{id}/summary и его части (сгенерированные типы). */
export type CourseSummaryTable = Schema<'CourseSummaryTable'>;
export type SummaryRow = Schema<'SummaryRow'>;
export type SummaryCell = Schema<'SummaryCell'>;

/** Сортировка сводной: `student` или `t:<test_id>` (по итоговому баллу, «нет попыток» — в конце). */
export function sortSummaryRows(rows: SummaryRow[], key: SortKey, dir: SortDir): SummaryRow[] {
	const value = (row: SummaryRow): number | string | null => {
		if (key === 'student') return row.display_name;
		const testId = key.startsWith('t:') ? key.slice(2) : key;
		const cell = row.cells.find((item) => item.test_id === testId);
		return cell && cell.attempts > 0 ? toNumber(cell.final_score) : null;
	};
	return rows
		.map((row, index) => ({ row, index }))
		.sort((a, b) => compareNullable(value(a.row), value(b.row), dir) || a.index - b.index)
		.map(({ row }) => row);
}

/** Следующее состояние сортировки по клику на заголовок. */
export function nextSort(current: { key: SortKey; dir: SortDir }, key: SortKey): { key: SortKey; dir: SortDir } {
	if (current.key !== key) return { key, dir: key === 'student' ? 'asc' : 'desc' };
	return { key, dir: current.dir === 'asc' ? 'desc' : 'asc' };
}

/** Обрезка текста вопроса для подсказки заголовка. */
export function shorten(text: string, max = 120): string {
	const plain = text.replace(/\s+/g, ' ').trim();
	return plain.length > max ? `${plain.slice(0, max - 1)}…` : plain;
}
