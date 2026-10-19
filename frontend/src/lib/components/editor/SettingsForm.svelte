<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { toast } from 'svelte-sonner';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import MskDateTimeInput from '$lib/components/app/MskDateTimeInput.svelte';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { selectClass } from './styles';

	type Settings = Schema<'TestSettings'>;

	/**
	 * Настройки теста. `locked` — тест заблокирован: менять можно только
	 * политику показа результатов и запрет копирования (spec course-authoring).
	 */
	let {
		testId,
		settings,
		questionCount,
		canEdit,
		locked,
		onsaved
	}: {
		testId: string;
		settings: Settings;
		questionCount: number;
		canEdit: boolean;
		locked: boolean;
		onsaved: () => Promise<void>;
	} = $props();

	const readonlyAll = $derived(!canEdit || locked);
	const readonlyReview = $derived(!canEdit);

	// Локальная форма; числа держим строками, чтобы поле можно было очистить.
	let unlimitedAttempts = $state(false);
	let attempts = $state('1');
	let noTimeLimit = $state(true);
	let minutes = $state('');
	let allQuestions = $state(true);
	let perAttempt = $state('');
	let freeNavigation = $state(false);
	let shuffleQuestions = $state(false);
	let shuffleOptions = $state(false);
	let reviewPolicy = $state<Schema<'ReviewMode'>>('after_submit');
	let reviewNotBefore = $state<string | null>(null);
	let finalPolicy = $state<Schema<'FinalScorePolicy'>>('best');
	let forbidCopy = $state(false);

	function reset(source: Settings): void {
		unlimitedAttempts = source.attempts_limit == null;
		attempts = String(source.attempts_limit ?? 1);
		noTimeLimit = source.time_limit_minutes == null;
		minutes = source.time_limit_minutes == null ? '' : String(source.time_limit_minutes);
		allQuestions = source.questions_per_attempt == null;
		perAttempt = source.questions_per_attempt == null ? '' : String(source.questions_per_attempt);
		freeNavigation = source.free_navigation;
		shuffleQuestions = source.shuffle_questions;
		shuffleOptions = source.shuffle_options;
		reviewPolicy = source.review_policy;
		reviewNotBefore = source.review_not_before ?? null;
		finalPolicy = source.final_policy;
		forbidCopy = source.copy_policy === 'forbidden';
	}

	$effect.pre(() => {
		reset(settings);
	});

	let saving = $state(false);
	let error = $state<string | null>(null);

	/** `bind:value` у `type="number"` отдаёт число или `null`, а не строку. */
	function positiveInt(value: string | number | null | undefined): number | null {
		const text = String(value ?? '').trim();
		return /^\d+$/.test(text) && Number(text) >= 1 ? Number(text) : null;
	}

	const problems = $derived.by(() => {
		const list: string[] = [];
		if (!unlimitedAttempts && positiveInt(attempts) === null) list.push('Лимит попыток — целое число от 1.');
		if (!noTimeLimit && positiveInt(minutes) === null) list.push('Лимит времени — целое число минут от 1.');
		if (!allQuestions) {
			const n = positiveInt(perAttempt);
			if (n === null) list.push('Число вопросов в попытке — целое число от 1.');
			else if (questionCount > 0 && n > questionCount) {
				list.push(`В тесте всего ${questionCount} вопросов — в попытке не может быть больше.`);
			}
		}
		return list;
	});

	function build(): Settings {
		if (locked) {
			// В заблокированном тесте отправляем прежние значения, меняются только показ результатов
			// и запрет копирования.
			return {
				...settings,
				review_policy: reviewPolicy,
				review_not_before: reviewPolicy === 'after_close' ? reviewNotBefore : null,
				copy_policy: copyPolicy()
			};
		}
		return {
			attempts_limit: unlimitedAttempts ? null : positiveInt(attempts),
			time_limit_minutes: noTimeLimit ? null : positiveInt(minutes),
			questions_per_attempt: allQuestions ? null : positiveInt(perAttempt),
			free_navigation: freeNavigation,
			shuffle_questions: shuffleQuestions,
			shuffle_options: shuffleOptions,
			review_policy: reviewPolicy,
			review_not_before: reviewPolicy === 'after_close' ? reviewNotBefore : null,
			final_policy: finalPolicy,
			copy_policy: copyPolicy()
		};
	}

	function copyPolicy(): Schema<'CopyPolicy'> {
		return forbidCopy ? 'forbidden' : 'allowed';
	}

	async function save(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		error = null;
		if (!locked && problems.length > 0) return;
		saving = true;
		try {
			await unwrap(
				api.PATCH('/api/tests/{test_id}', {
					params: { path: { test_id: testId } },
					body: { settings: build() }
				})
			);
			toast.success('Настройки сохранены');
			await onsaved();
		} catch (err) {
			error = errorMessage(err);
		} finally {
			saving = false;
		}
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title>Настройки</Card.Title>
		{#if locked && canEdit}
			<Card.Description>Тест заблокирован: сейчас можно менять только показ результатов и запрет копирования.</Card.Description>
		{/if}
	</Card.Header>
	<Card.Content>
		<form class="grid gap-6" novalidate onsubmit={save}>
			<div class="grid gap-6 sm:grid-cols-2">
				<div class="grid content-start gap-2">
					<Label for="attempts">Попыток на студента</Label>
					<div class="flex items-center gap-3">
						<Input
							id="attempts"
							type="number"
							min="1"
							inputmode="numeric"
							class="w-24"
							bind:value={attempts}
							disabled={readonlyAll || unlimitedAttempts}
						/>
						<label class="flex items-center gap-2 text-sm">
							<Checkbox bind:checked={unlimitedAttempts} disabled={readonlyAll} />
							без ограничения
						</label>
					</div>
				</div>

				<div class="grid content-start gap-2">
					<Label for="minutes">Лимит времени на попытку</Label>
					<div class="flex flex-wrap items-center gap-3">
						<div class="flex items-center gap-2">
							<Input
								id="minutes"
								type="number"
								min="1"
								inputmode="numeric"
								class="w-24"
								bind:value={minutes}
								disabled={readonlyAll || noTimeLimit}
							/>
							<span class="text-muted-foreground text-sm">мин</span>
						</div>
						<label class="flex items-center gap-2 text-sm">
							<Checkbox bind:checked={noTimeLimit} disabled={readonlyAll} />
							без лимита
						</label>
					</div>
				</div>

				<div class="grid content-start gap-2">
					<Label for="per-attempt">Вопросов в попытке</Label>
					<div class="flex items-center gap-3">
						<Input
							id="per-attempt"
							type="number"
							min="1"
							inputmode="numeric"
							class="w-24"
							bind:value={perAttempt}
							disabled={readonlyAll || allQuestions}
						/>
						<label class="flex items-center gap-2 text-sm">
							<Checkbox bind:checked={allQuestions} disabled={readonlyAll} />
							все ({questionCount})
						</label>
					</div>
					<p class="text-muted-foreground text-xs">
						Если меньше, чем вопросов в тесте, каждая попытка получит свою случайную выборку.
					</p>
				</div>

				<div class="grid content-start gap-2">
					<Label for="final">Итоговая попытка</Label>
					<select id="final" class={selectClass} bind:value={finalPolicy} disabled={readonlyAll}>
						<option value="best">Лучшая</option>
						<option value="last">Последняя</option>
						<option value="average">Средний балл</option>
					</select>
				</div>
			</div>

			<div class="grid gap-3">
				<label class="flex items-start gap-3 text-sm">
					<Checkbox bind:checked={freeNavigation} disabled={readonlyAll} class="mt-0.5" />
					<span>
						<span class="font-medium">Свободная навигация</span>
						<span class="text-muted-foreground block text-xs">
							Можно возвращаться к предыдущим вопросам и пропускать вопросы. Если выключено — вопросы
							идут по одному, ответ фиксируется при переходе дальше.
						</span>
					</span>
				</label>
				<label class="flex items-start gap-3 text-sm">
					<Checkbox bind:checked={shuffleQuestions} disabled={readonlyAll} class="mt-0.5" />
					<span class="font-medium">Перемешивать вопросы</span>
				</label>
				<label class="flex items-start gap-3 text-sm">
					<Checkbox bind:checked={shuffleOptions} disabled={readonlyAll} class="mt-0.5" />
					<span>
						<span class="font-medium">Перемешивать варианты ответа</span>
						<span class="text-muted-foreground block text-xs">Закреплённые варианты остаются на месте.</span>
					</span>
				</label>
				<label class="flex items-start gap-3 text-sm">
					<Checkbox bind:checked={forbidCopy} disabled={readonlyReview} class="mt-0.5" />
					<span>
						<span class="font-medium">Запретить копирование текста</span>
						<span class="text-muted-foreground block text-xs">
							Студент не сможет выделить и скопировать вопросы и варианты при прохождении и в разборе. В поле
							свободного ответа копирование и вставка работают. Мешает обычному копированию, но не защищает
							от инструментов разработчика браузера.
						</span>
					</span>
				</label>
			</div>

			<div class="grid gap-4 sm:grid-cols-2">
				<div class="grid content-start gap-2">
					<Label for="review">Показ результатов студенту</Label>
					<select id="review" class={selectClass} bind:value={reviewPolicy} disabled={readonlyReview}>
						<option value="after_submit">Сразу после сдачи</option>
						<option value="after_close">После закрытия теста</option>
						<option value="never">Никогда (только факт сдачи)</option>
					</select>
					{#if reviewPolicy === 'after_close'}
						<p class="text-muted-foreground text-xs">
							Разбор откроется, когда окно потока закрыто и все попытки потока завершены.
						</p>
					{/if}
				</div>
				{#if reviewPolicy === 'after_close'}
					<MskDateTimeInput
						id="review-not-before"
						label="Не раньше (необязательно)"
						bind:value={reviewNotBefore}
						disabled={readonlyReview}
						hint="Пусто — сразу после закрытия."
					/>
				{/if}
			</div>

			{#if (!locked && problems.length > 0) || error}
				<Alert.Root variant="destructive" aria-live="polite">
					<CircleAlertIcon />
					<Alert.Description>
						<ul class="list-disc pl-4">
							{#if !locked}{#each problems as problem (problem)}<li>{problem}</li>{/each}{/if}
							{#if error}<li>{error}</li>{/if}
						</ul>
					</Alert.Description>
				</Alert.Root>
			{/if}

			{#if canEdit}
				<div class="flex flex-wrap gap-2">
					<Button type="submit" disabled={saving || (!locked && problems.length > 0)}>
						{#if saving}<LoaderIcon class="animate-spin" />Сохраняем…{:else}Сохранить настройки{/if}
					</Button>
					<Button variant="ghost" onclick={() => reset(settings)} disabled={saving}>Сбросить</Button>
				</div>
			{/if}
		</form>
	</Card.Content>
</Card.Root>
