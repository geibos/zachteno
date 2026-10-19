<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import TimerIcon from '@lucide/svelte/icons/timer';
	import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import LockIcon from '@lucide/svelte/icons/lock';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import QuestionView from './QuestionView.svelte';
	import { api, errorMessage, toApiError, unwrap, type Schema } from '$lib/api/client';
	import { SaveQueue, type PendingAnswer, type SaveStatus, type SendResult } from '$lib/attempt/save-queue';
	import { createPendingStore } from '$lib/attempt/idb-store';
	import { EventBatcher, type ClientEventKind } from '$lib/attempt/events';
	import { clockOffset, LOW_TIME_MS, remainingMs } from '$lib/attempt/clock';
	import { EMPTY_ANSWER, fromSaved, isAnswered, toPayload, type LocalAnswer } from '$lib/attempt/answers';
	import { count, formatDuration } from '$lib/format';
	import { cn } from '$lib/utils';
	import { sendSignals, TypingRecorder, typingRhythm } from '$lib/signals';
	import { noCopy } from '$lib/attempt/no-copy';

	let { initial }: { initial: Schema<'AttemptView'> } = $props();

	// Попытка не меняет состав и порядок вопросов — берём их один раз
	// (страница пересоздаёт раннер через {#key} при смене попытки).
	// В последовательном режиме сервер отдаёт только текущий вопрос: следующий приходит после перехода.
	const view = untrack(() => initial);
	const attemptId = view.id;
	const byIndex = (list: Schema<'RunnerQuestion'>[]) => [...list].sort((a, b) => a.index - b.index);
	let questions = $state(byIndex(view.questions));
	const total = view.question_count;
	const sequential = view.navigation === 'sequential';

	let answers = $state<Record<string, LocalAnswer>>(fromSaved(view.answers));
	let statuses = $state<Record<string, { status: SaveStatus; message?: string }>>({});
	let unsavedCount = $state(0);
	let online = $state(typeof navigator === 'undefined' ? true : navigator.onLine);
	let current = $state(Math.min(Math.max(view.current_index, 0), Math.max(total - 1, 0)));
	let now = $state(Date.now());
	let finishing = $state<null | 'manual' | 'timeout'>(null);
	let submitOpen = $state(false);
	let navPending = $state(false);

	let offset = clockOffset(view.server_now, Date.now());
	const remaining = $derived(remainingMs(view.deadline_at, offset, now));
	const question = $derived(questions.find((q) => q.index === current));
	const unanswered = $derived(questions.filter((q) => !isAnswered(q.choice, answers[q.id])));
	const timeOver = $derived(remaining === 0);

	// --- сохранение ответов ---

	async function send(entry: PendingAnswer): Promise<SendResult> {
		const { response, error } = await api.PUT('/api/attempts/{attempt_id}/answers/{question_id}', {
			params: { path: { attempt_id: attemptId, question_id: entry.questionId } },
			body: {
				client_seq: entry.client_seq,
				selected_option_ids: entry.selected_option_ids,
				text: entry.text
			}
		});
		if (response.ok) return { kind: 'ok' };
		const apiError = toApiError(response, error);
		if (response.status === 409) return { kind: 'conflict', message: apiError.message };
		if (response.status >= 500 || response.status === 429 || response.status === 408) return { kind: 'retry' };
		return { kind: 'rejected', message: apiError.message };
	}

	const queue = new SaveQueue({
		attemptId,
		store: createPendingStore(),
		send,
		onStatus: (questionId, status, message) => {
			statuses[questionId] = { status, message };
			unsavedCount = queue.unsavedCount;
		},
		onConflict: (message) => void handleConflict(message)
	});

	/** 409 при сохранении: время вышло, попытка сдана или вопрос уже закрыт. */
	async function handleConflict(message: string): Promise<void> {
		if (finishing) return;
		try {
			const fresh = await unwrap(
				api.GET('/api/attempts/{attempt_id}', { params: { path: { attempt_id: attemptId } } })
			);
			offset = clockOffset(fresh.server_now, Date.now());
			const left = remainingMs(fresh.deadline_at, offset, Date.now());
			if (fresh.submitted_at || left === 0) {
				queue.stop();
				toast.info(fresh.submitted_at ? 'Попытка уже сдана' : 'Время вышло');
				await goto(`/attempts/${attemptId}/review`, { replaceState: true });
				return;
			}
			if (sequential) {
				questions = byIndex(fresh.questions);
				current = Math.min(fresh.current_index, total - 1);
			}
		} catch {
			// нет связи — покажем исходное сообщение
		}
		toast.error(message);
	}

	// Ритм набора — только в полях свободного ответа.
	const typing = new TypingRecorder();
	const isFreeTextField = (target: EventTarget) => (target as Element).tagName === 'TEXTAREA';

	function change(q: Schema<'RunnerQuestion'>, answer: LocalAnswer, debounce: boolean): void {
		if (finishing || timeOver) return;
		answers[q.id] = answer;
		queue.set(q.id, toPayload(q.choice, answer), debounce ? 800 : 0);
		// Сигнал «answer» — не чаще раза в минуту; при пропуске ритм копится дальше.
		if (!view.is_preview && sendSignals('answer', typing.snapshot())) typing.reset();
	}

	// --- события вкладки ---

	const events = new EventBatcher(async (batch: ClientEventKind[], keepalive: boolean) => {
		const response = await fetch(`/api/attempts/${attemptId}/events`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ events: batch }),
			credentials: 'same-origin',
			keepalive
		});
		if (!response.ok && response.status >= 500) throw new Error(`events: ${response.status}`);
	});

	// --- сдача ---

	function delay(ms: number): Promise<void> {
		return new Promise((resolve) => setTimeout(resolve, ms));
	}

	async function finish(kind: 'manual' | 'timeout'): Promise<void> {
		if (finishing) return;
		finishing = kind;
		submitOpen = false;
		// Досылаем сохранённое, но не ждём бесконечно.
		await Promise.race([queue.flushAll(), delay(kind === 'timeout' ? 3000 : 8000)]);
		try {
			await unwrap(api.POST('/api/attempts/{attempt_id}/submit', { params: { path: { attempt_id: attemptId } } }));
		} catch (error) {
			const status = (error as { status?: number }).status;
			// 409 — уже сдана или время вышло: результат всё равно на странице разбора.
			if (kind === 'manual' && status !== 409) {
				finishing = null;
				toast.error(`Не удалось сдать попытку: ${errorMessage(error)}`);
				return;
			}
		}
		queue.stop();
		void events.flush(true);
		await goto(`/attempts/${attemptId}/review`, { replaceState: true });
	}

	$effect(() => {
		if (timeOver && !finishing) void finish('timeout');
	});

	// --- последовательный режим ---

	async function goNext(): Promise<void> {
		const q = question;
		if (!q || !isAnswered(q.choice, answers[q.id]) || navPending) return;
		navPending = true;
		try {
			const saved = await queue.settle(q.id);
			if (!saved || statuses[q.id]?.status === 'error') {
				toast.error('Ответ ещё не сохранён. Проверьте подключение и попробуйте снова.');
				return;
			}
			const moved = await unwrap(
				api.POST('/api/attempts/{attempt_id}/navigate', {
					params: { path: { attempt_id: attemptId } },
					body: { to_index: current + 1 }
				})
			);
			questions = byIndex(moved.questions);
			current = moved.current_index;
			window.scrollTo({ top: 0, behavior: 'smooth' });
		} catch (error) {
			toast.error(errorMessage(error));
			if ((error as { status?: number }).status === 409) void handleConflict(errorMessage(error));
		} finally {
			navPending = false;
		}
	}

	// --- свободный режим ---

	function goTo(index: number): void {
		const leaving = questions[current];
		if (leaving) void queue.settle(leaving.id);
		current = index;
		submitOpen = false;
		window.scrollTo({ top: 0, behavior: 'smooth' });
	}

	onMount(() => {
		const tick = setInterval(() => (now = Date.now()), 500);

		void queue.restore(view.answers).then((restored) => {
			for (const entry of restored) {
				answers[entry.questionId] = { selected: [...entry.selected_option_ids], text: entry.text ?? '' };
			}
			unsavedCount = queue.unsavedCount;
		});

		const onVisibility = () => {
			if (document.visibilityState === 'hidden') {
				events.push('tab_hidden');
				void events.flush(true);
			} else {
				events.push('tab_visible');
			}
		};
		const onOnline = () => {
			online = true;
			events.push('online');
			void queue.flushAll();
		};
		const onOffline = () => {
			online = false;
			events.push('offline');
		};
		const onBeforeUnload = (event: BeforeUnloadEvent) => {
			if (queue.unsavedCount > 0 && !finishing) event.preventDefault();
		};
		const onPageHide = () => void events.flush(true);

		document.addEventListener('visibilitychange', onVisibility);
		window.addEventListener('online', onOnline);
		window.addEventListener('offline', onOffline);
		window.addEventListener('beforeunload', onBeforeUnload);
		window.addEventListener('pagehide', onPageHide);
		return () => {
			clearInterval(tick);
			document.removeEventListener('visibilitychange', onVisibility);
			window.removeEventListener('online', onOnline);
			window.removeEventListener('offline', onOffline);
			window.removeEventListener('beforeunload', onBeforeUnload);
			window.removeEventListener('pagehide', onPageHide);
			void events.flush(true);
			events.dispose();
		};
	});
</script>

<div
	class="mx-auto grid w-full max-w-3xl grid-cols-1 gap-4 px-4 pt-4 pb-24 sm:px-6"
	use:typingRhythm={{ recorder: typing, accept: isFreeTextField }}
	use:noCopy={{
		enabled: view.forbid_copy,
		onblocked: () => toast.info('Копирование в этом тесте отключено', { id: 'no-copy' })
	}}
>
	{#if view.is_preview}
		<Alert.Root class="border-primary/30 bg-primary/5">
			<EyeIcon class="text-primary" />
			<Alert.Title>Предпросмотр</Alert.Title>
			<Alert.Description>
				Вы проходите тест как студент. Попытка не попадёт в результаты и не заблокирует редактирование.
				<a href="/tests/{view.test_id}" class="text-primary underline underline-offset-3">Вернуться к тесту</a>
			</Alert.Description>
		</Alert.Root>
	{/if}

	<!-- Панель попытки: под шапкой сайта (h-14). -->
	<div
		class="bg-background/95 supports-[backdrop-filter]:bg-background/80 sticky top-14 z-30 -mx-4 flex items-center gap-3 border-b px-4 py-2 backdrop-blur sm:-mx-6 sm:px-6"
	>
		<p class="min-w-0 flex-1 truncate font-medium" title={view.title}>{view.title}</p>
		{#if remaining !== null}
			<span
				role="timer"
				aria-label="Осталось времени"
				class={cn(
					'inline-flex items-center gap-1 rounded-md px-2 py-1 font-mono text-sm tabular-nums',
					remaining <= LOW_TIME_MS ? 'bg-destructive/10 text-destructive' : 'bg-muted'
				)}
			>
				<TimerIcon class="size-4" />
				{formatDuration(remaining)}
			</span>
		{/if}
		{#if !sequential}
			<Button size="sm" onclick={() => (submitOpen = true)} disabled={finishing !== null}>Сдать</Button>
		{/if}
	</div>

	{#if unsavedCount > 0 && (!online || Object.values(statuses).some((s) => s.status === 'unsaved'))}
		<Alert.Root class="border-amber-500/40 bg-amber-50 text-amber-950 dark:bg-amber-950/30 dark:text-amber-100">
			<CloudOffIcon />
			<Alert.Title>Есть несохранённые ответы ({unsavedCount})</Alert.Title>
			<Alert.Description class="text-current/80">
				{online
					? 'Повторяем отправку. Не закрывайте вкладку, пока ответы не сохранятся.'
					: 'Нет связи с интернетом. Ответы сохранены на этом устройстве и отправятся, когда связь вернётся — не закрывайте вкладку.'}
			</Alert.Description>
		</Alert.Root>
	{/if}

	{#if !sequential}
		<nav aria-label="Вопросы" class="flex flex-wrap gap-1.5">
			{#each questions as q, i (q.id)}
				{@const answered = isAnswered(q.choice, answers[q.id])}
				{@const st = statuses[q.id]?.status}
				<button
					type="button"
					onclick={() => goTo(i)}
					aria-current={i === current ? 'step' : undefined}
					aria-label="Вопрос {i + 1}{answered ? ', есть ответ' : ', без ответа'}"
					class={cn(
						'relative flex size-9 items-center justify-center rounded-md border text-sm font-medium transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
						answered ? 'border-primary/40 bg-primary/10 text-primary' : 'bg-background hover:bg-muted',
						i === current && 'ring-primary ring-2 ring-offset-1 ring-offset-background'
					)}
				>
					{i + 1}
					{#if st === 'unsaved' || st === 'error'}
						<span class="absolute -top-1 -right-1 size-2.5 rounded-full bg-amber-500" aria-hidden="true"></span>
					{/if}
				</button>
			{/each}
		</nav>
	{/if}

	{#if question}
		<Card.Root>
			<Card.Content>
				<QuestionView
					{question}
					number={current + 1}
					{total}
					answer={answers[question.id] ?? EMPTY_ANSWER}
					status={statuses[question.id]?.status ?? (isAnswered(question.choice, answers[question.id]) ? 'saved' : null)}
					statusMessage={statuses[question.id]?.message}
					disabled={finishing !== null || timeOver}
					onchange={(answer, debounce) => change(question, answer, debounce)}
					ontextblur={() => void queue.settle(question.id)}
				/>
			</Card.Content>
		</Card.Root>

		{#if sequential}
			<div class="grid gap-2">
				<p class="text-muted-foreground flex items-center gap-1.5 text-xs">
					<LockIcon class="size-3.5" />Вопросы идут по порядку: после перехода изменить ответ нельзя.
				</p>
				{#if current < total - 1}
					<Button
						size="lg"
						onclick={goNext}
						disabled={!isAnswered(question.choice, answers[question.id]) || navPending || finishing !== null}
					>
						{#if navPending}<LoaderIcon class="animate-spin" />{/if}
						Далее
						<ChevronRightIcon />
					</Button>
					{#if !isAnswered(question.choice, answers[question.id])}
						<p class="text-muted-foreground text-center text-xs">Чтобы перейти дальше, ответьте на вопрос.</p>
					{/if}
				{:else}
					<Button
						size="lg"
						onclick={() => (submitOpen = true)}
						disabled={!isAnswered(question.choice, answers[question.id]) || finishing !== null}
					>
						Сдать попытку
					</Button>
				{/if}
			</div>
		{:else}
			<div class="flex items-center justify-between gap-2">
				<Button variant="outline" onclick={() => goTo(current - 1)} disabled={current === 0}>
					<ChevronLeftIcon />Назад
				</Button>
				{#if current < total - 1}
					<Button variant="outline" onclick={() => goTo(current + 1)}>
						Далее<ChevronRightIcon />
					</Button>
				{:else}
					<Button onclick={() => (submitOpen = true)} disabled={finishing !== null}>Сдать</Button>
				{/if}
			</div>
		{/if}
	{:else}
		<p class="text-muted-foreground">В попытке нет вопросов.</p>
	{/if}
</div>

<Dialog.Root bind:open={submitOpen}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Сдать попытку?</Dialog.Title>
			<Dialog.Description>После сдачи изменить ответы будет нельзя.</Dialog.Description>
		</Dialog.Header>
		{#if !sequential && unanswered.length > 0}
			<Alert.Root class="border-amber-500/40 bg-amber-50 text-amber-950 dark:bg-amber-950/30 dark:text-amber-100">
				<TriangleAlertIcon />
				<Alert.Title>Без ответа {count(unanswered.length, ['вопрос', 'вопроса', 'вопросов'])}</Alert.Title>
				<Alert.Description class="text-current/80">
					<div class="mt-1 flex flex-wrap gap-1.5">
						{#each unanswered as q (q.id)}
							{@const index = questions.indexOf(q)}
							<button
								type="button"
								class="bg-background rounded border px-2 py-0.5 text-sm font-medium underline-offset-3 hover:underline"
								onclick={() => goTo(index)}
							>
								№ {index + 1}
							</button>
						{/each}
					</div>
				</Alert.Description>
			</Alert.Root>
		{/if}
		{#if unsavedCount > 0}
			<p class="text-sm text-amber-700 dark:text-amber-300">
				{count(unsavedCount, ['ответ', 'ответа', 'ответов'])} ещё не сохранено на сервере. Перед сдачей попробуем
				отправить их ещё раз; без связи они не будут учтены.
			</p>
		{/if}
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (submitOpen = false)}>Вернуться к тесту</Button>
			<Button onclick={() => finish('manual')} disabled={finishing !== null}>
				{!sequential && unanswered.length > 0 ? 'Сдать с пропусками' : 'Сдать'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

{#if finishing}
	<div class="bg-background/80 fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm" role="alertdialog" aria-live="assertive" aria-label="Сдача попытки">
		<Card.Root class="w-full max-w-sm">
			<Card.Content class="flex flex-col items-center gap-3 py-6 text-center">
				<LoaderIcon class="text-primary size-8 animate-spin" />
				<p class="font-medium">{finishing === 'timeout' ? 'Время вышло — сдаём попытку…' : 'Сдаём попытку…'}</p>
			</Card.Content>
		</Card.Root>
	</div>
{/if}
