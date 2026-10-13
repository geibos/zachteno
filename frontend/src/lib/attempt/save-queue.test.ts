import { describe, expect, it } from 'vitest';
import {
	MemoryStore,
	SaveQueue,
	SeqClock,
	type PendingAnswer,
	type SaveStatus,
	type SendResult
} from './save-queue';

/** Ручные таймеры: тест сам решает, когда «прошло время». */
function manualTimers() {
	let id = 0;
	const timers = new Map<number, { fn: () => void; ms: number }>();
	return {
		setTimer: (fn: () => void, ms: number) => {
			id += 1;
			timers.set(id, { fn, ms });
			return id;
		},
		clearTimer: (handle: unknown) => {
			timers.delete(handle as number);
		},
		pending: () => [...timers.values()],
		runAll: () => {
			const all = [...timers.entries()];
			timers.clear();
			for (const [, timer] of all) timer.fn();
		}
	};
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup(options: { online?: () => boolean; send?: (e: PendingAnswer) => Promise<SendResult> } = {}) {
	const sent: PendingAnswer[] = [];
	const statuses: Array<[string, SaveStatus]> = [];
	const conflicts: string[] = [];
	const store = new MemoryStore();
	const timers = manualTimers();
	let now = 1_000;
	const queue = new SaveQueue({
		attemptId: 'a1',
		store,
		send: async (entry) => {
			sent.push(entry);
			return options.send ? options.send(entry) : { kind: 'ok' };
		},
		onStatus: (questionId, status) => statuses.push([questionId, status]),
		onConflict: (message) => conflicts.push(message),
		now: () => now,
		isOnline: options.online ?? (() => true),
		setTimer: timers.setTimer,
		clearTimer: timers.clearTimer
	});
	return { queue, sent, statuses, conflicts, store, timers, setNow: (value: number) => (now = value) };
}

describe('SeqClock', () => {
	it('строго растёт, даже если часы стоят или идут назад', () => {
		let now = 500;
		const clock = new SeqClock(() => now);
		const a = clock.next();
		const b = clock.next();
		now = 100;
		const c = clock.next();
		expect(a).toBe(500);
		expect(b).toBe(501);
		expect(c).toBe(502);
	});

	it('не опускается ниже уже виденных номеров', () => {
		const clock = new SeqClock(() => 10);
		clock.observe(9_999);
		expect(clock.next()).toBe(10_000);
	});
});

describe('SaveQueue', () => {
	it('client_seq растёт с каждым изменением', async () => {
		const { queue, sent } = setup();
		queue.set('q1', { selected_option_ids: ['o1'], text: null });
		await tick();
		queue.set('q1', { selected_option_ids: ['o2'], text: null });
		await tick();
		queue.set('q2', { selected_option_ids: [], text: 'ответ' });
		await tick();
		const seqs = sent.map((entry) => entry.client_seq);
		expect(seqs).toHaveLength(3);
		expect(seqs[1]).toBeGreaterThan(seqs[0]!);
		expect(seqs[2]).toBeGreaterThan(seqs[1]!);
		expect(queue.unsavedCount).toBe(0);
	});

	it('без сети копит ответ в хранилище и отправляет после online', async () => {
		let online = false;
		const { queue, sent, store, timers, statuses } = setup({ online: () => online });
		queue.set('q1', { selected_option_ids: ['o1'], text: null });
		await tick();
		expect(sent).toHaveLength(0);
		expect(queue.unsavedCount).toBe(1);
		expect(store.entries.size).toBe(1);
		expect(statuses.at(-1)).toEqual(['q1', 'unsaved']);

		online = true;
		await queue.flushAll();
		expect(sent).toHaveLength(1);
		expect(sent[0]!.selected_option_ids).toEqual(['o1']);
		expect(queue.unsavedCount).toBe(0);
		await tick();
		expect(store.entries.size).toBe(0);
		expect(statuses.at(-1)).toEqual(['q1', 'saved']);
		timers.runAll();
	});

	it('при сетевой ошибке повторяет отправку по таймеру', async () => {
		let fail = true;
		const { queue, sent, timers } = setup({
			send: async () => (fail ? { kind: 'retry' } : { kind: 'ok' })
		});
		queue.set('q1', { selected_option_ids: ['o1'], text: null });
		await tick();
		expect(sent).toHaveLength(1);
		expect(queue.unsavedCount).toBe(1);
		expect(timers.pending()).toHaveLength(1);

		fail = false;
		timers.runAll();
		await tick();
		expect(sent).toHaveLength(2);
		expect(sent[1]!.client_seq).toBe(sent[0]!.client_seq);
		expect(queue.unsavedCount).toBe(0);
	});

	it('после восстановления из хранилища новые номера больше сохранённых', async () => {
		const first = setup({ online: () => false });
		first.setNow(5_000);
		first.queue.set('q1', { selected_option_ids: ['o1'], text: null });
		await tick();

		// «Перезагрузка вкладки»: новая очередь с тем же хранилищем и отстающими часами.
		const sent: PendingAnswer[] = [];
		const queue = new SaveQueue({
			attemptId: 'a1',
			store: first.store,
			send: async (entry) => {
				sent.push(entry);
				return { kind: 'ok' };
			},
			onStatus: () => undefined,
			now: () => 10,
			isOnline: () => true,
			setTimer: first.timers.setTimer,
			clearTimer: first.timers.clearTimer
		});
		const restored = await queue.restore([{ question_id: 'q1', client_seq: 4_000 }]);
		expect(restored.map((e) => e.selected_option_ids)).toEqual([['o1']]);
		await tick();
		expect(sent.map((e) => e.client_seq)).toEqual([5_000]);
		queue.set('q1', { selected_option_ids: ['o2'], text: null });
		await tick();
		expect(sent.at(-1)!.client_seq).toBeGreaterThan(5_000);
	});

	it('дебаунс текста отправляет только последнее значение', async () => {
		const { queue, sent, timers } = setup();
		queue.set('q1', { selected_option_ids: [], text: 'о' }, 800);
		queue.set('q1', { selected_option_ids: [], text: 'отв' }, 800);
		queue.set('q1', { selected_option_ids: [], text: 'ответ' }, 800);
		await tick();
		expect(sent).toHaveLength(0);
		timers.runAll();
		await tick();
		expect(sent).toHaveLength(1);
		expect(sent[0]!.text).toBe('ответ');
	});

	it('409 останавливает повторы и сообщает наверх', async () => {
		const { queue, conflicts, timers } = setup({
			send: async () => ({ kind: 'conflict', message: 'Попытка завершена или время вышло' })
		});
		queue.set('q1', { selected_option_ids: ['o1'], text: null });
		await tick();
		expect(conflicts).toEqual(['Попытка завершена или время вышло']);
		expect(queue.unsavedCount).toBe(0);
		expect(timers.pending()).toHaveLength(0);
	});
});
