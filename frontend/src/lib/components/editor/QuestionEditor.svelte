<script lang="ts">
	import { untrack } from 'svelte';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import PinIcon from '@lucide/svelte/icons/pin';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { errorMessage } from '$lib/api/client';
	import ImageField from './ImageField.svelte';
	import MarkdownField from './MarkdownField.svelte';
	import { selectClass } from './styles';
	import {
		KIND_HINTS,
		KIND_LABELS,
		changeKind,
		emptyOption,
		isMultiple,
		validateDraft,
		type QuestionDraft,
		type QuestionKind
	} from './question-model';

	/**
	 * Форма вопроса целиком: тип, текст, вес, картинка, варианты.
	 * `onsave` получает черновик; ошибку сервера показываем здесь же.
	 */
	let {
		draft: initial,
		number,
		onsave,
		oncancel
	}: {
		draft: QuestionDraft;
		number: number | null;
		onsave: (draft: QuestionDraft) => Promise<void>;
		oncancel: () => void;
	} = $props();

	// Черновик — локальная копия: отмена не трогает данные страницы.
	let draft = $state<QuestionDraft>(untrack(() => $state.snapshot(initial) as QuestionDraft));
	let submitted = $state(false);
	let saving = $state(false);
	let serverError = $state<string | null>(null);

	const errors = $derived(submitted ? validateDraft(draft) : []);
	const multiple = $derived(isMultiple(draft.kind));
	const uid = Math.random().toString(36).slice(2, 8);

	function setKind(kind: QuestionKind): void {
		draft = changeKind(draft, kind);
	}

	function markCorrect(index: number, checked: boolean): void {
		if (draft.kind === 'single') {
			draft.options.forEach((option, i) => (option.is_correct = checked && i === index));
		} else {
			const option = draft.options[index];
			if (option) option.is_correct = checked;
		}
	}

	function move(index: number, delta: number): void {
		const target = index + delta;
		if (target < 0 || target >= draft.options.length) return;
		const options = [...draft.options];
		const [item] = options.splice(index, 1);
		if (item) options.splice(target, 0, item);
		draft.options = options;
	}

	async function save(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		serverError = null;
		if (validateDraft(draft).length > 0) return;
		saving = true;
		try {
			await onsave($state.snapshot(draft) as QuestionDraft);
		} catch (error) {
			serverError = errorMessage(error);
		} finally {
			saving = false;
		}
	}
</script>

<Card.Root class="border-primary/40 ring-primary/10 ring-2">
	<Card.Header>
		<Card.Title>{number === null ? 'Новый вопрос' : `Вопрос ${number}`}</Card.Title>
	</Card.Header>
	<Card.Content>
		<form class="grid gap-5" novalidate onsubmit={save}>
			<div class="grid gap-4 sm:grid-cols-[1fr_8rem]">
				<div class="grid gap-2">
					<Label for="kind-{uid}">Тип вопроса</Label>
					<select
						id="kind-{uid}"
						class={selectClass}
						value={draft.kind}
						onchange={(e) => setKind(e.currentTarget.value as QuestionKind)}
					>
						{#each Object.entries(KIND_LABELS) as [kind, label] (kind)}
							<option value={kind}>{label}</option>
						{/each}
					</select>
					<p class="text-muted-foreground text-xs">{KIND_HINTS[draft.kind]}</p>
				</div>
				<div class="grid content-start gap-2">
					<Label for="weight-{uid}">Вес</Label>
					<Input id="weight-{uid}" inputmode="decimal" bind:value={draft.weight} placeholder="1" />
				</div>
			</div>

			<MarkdownField
				id="body-{uid}"
				label="Текст вопроса"
				rows={4}
				placeholder="Можно использовать Markdown и формулы: $x^2$, $$\int_0^1 x\,dx$$"
				bind:value={draft.body}
			/>

			<div class="grid gap-2">
				<span class="text-sm font-medium">Картинка к вопросу</span>
				<ImageField id="qimg-{uid}" bind:image={draft.image} />
			</div>

			{#if draft.kind !== 'free_text'}
				<fieldset class="grid gap-3">
					<legend class="mb-2 text-sm font-medium">Варианты ответа</legend>
					<p class="text-muted-foreground -mt-1 text-xs">
						{draft.kind === 'single' ? 'Отметьте один верный вариант.' : 'Отметьте все верные варианты.'}
						Штраф — число ≤ 0 за выбор неверного варианта; пусто — по умолчанию
						{draft.kind === 'multiple_any' ? '(−вес вопроса)' : '(0)'}. Закреплённый вариант не
						перемешивается.
					</p>
					{#each draft.options as option, index (option.key)}
						<div class="bg-muted/30 grid gap-3 rounded-lg border p-3">
							<div class="flex flex-wrap items-center gap-x-4 gap-y-2">
								<span class="text-muted-foreground text-sm font-medium">{index + 1}.</span>
								<label class="flex items-center gap-2 text-sm">
									{#if draft.kind === 'single'}
										<input
											type="radio"
											name="correct-{uid}"
											class="accent-primary size-4"
											checked={option.is_correct}
											onchange={() => markCorrect(index, true)}
										/>
									{:else}
										<Checkbox
											checked={option.is_correct}
											onCheckedChange={(checked) => markCorrect(index, checked === true)}
										/>
									{/if}
									Верный
								</label>
								<label class="flex items-center gap-2 text-sm">
									<Checkbox bind:checked={option.pinned} />
									<PinIcon class="size-3.5" />
									Закрепить
								</label>
								<div class="ml-auto flex items-center gap-1">
									<Button
										size="icon-sm"
										variant="ghost"
										aria-label="Выше"
										disabled={index === 0}
										onclick={() => move(index, -1)}
									>
										<ArrowUpIcon />
									</Button>
									<Button
										size="icon-sm"
										variant="ghost"
										aria-label="Ниже"
										disabled={index === draft.options.length - 1}
										onclick={() => move(index, 1)}
									>
										<ArrowDownIcon />
									</Button>
									<Button
										size="icon-sm"
										variant="ghost"
										aria-label="Удалить вариант"
										onclick={() => (draft.options = draft.options.filter((o) => o.key !== option.key))}
									>
										<TrashIcon />
									</Button>
								</div>
							</div>
							<MarkdownField
								id="opt-{uid}-{option.key}"
								label="Текст варианта {index + 1}"
								hideLabel
								rows={2}
								placeholder="Текст варианта (можно оставить пустым, если есть картинка)"
								bind:value={option.body}
							/>
							<div class="flex flex-col gap-3 sm:flex-row sm:items-start">
								<div class="min-w-0 flex-1">
									<ImageField id="oimg-{uid}-{option.key}" bind:image={option.image} compact />
								</div>
								<div class="flex items-center gap-2 sm:w-44">
									<Label for="pen-{uid}-{option.key}" class="shrink-0 text-xs">Штраф</Label>
									<Input
										id="pen-{uid}-{option.key}"
										inputmode="decimal"
										placeholder={draft.kind === 'multiple_any' ? '−вес' : '0'}
										bind:value={option.penalty}
										class="h-8"
									/>
								</div>
							</div>
						</div>
					{/each}
					<div>
						<Button
							size="sm"
							variant="outline"
							onclick={() => (draft.options = [...draft.options, emptyOption()])}
						>
							<PlusIcon />
							Добавить вариант
						</Button>
					</div>
					{#if multiple}
						<p class="text-muted-foreground text-xs">Для множественного выбора нужно минимум два варианта.</p>
					{/if}
				</fieldset>
			{:else}
				<p class="text-muted-foreground rounded-md border border-dashed p-3 text-sm">
					У вопроса со свободным ответом нет вариантов. Ответ студента попадёт в результаты с пометкой
					«не оценивается».
				</p>
			{/if}

			{#if errors.length > 0 || serverError}
				<Alert.Root variant="destructive" aria-live="polite">
					<CircleAlertIcon />
					<Alert.Title>Проверьте вопрос</Alert.Title>
					<Alert.Description>
						<ul class="list-disc pl-4">
							{#each errors as error (error)}<li>{error}</li>{/each}
							{#if serverError}<li>{serverError}</li>{/if}
						</ul>
					</Alert.Description>
				</Alert.Root>
			{/if}

			<div class="flex flex-wrap gap-2">
				<Button type="submit" disabled={saving}>
					{#if saving}<LoaderIcon class="animate-spin" />Сохраняем…{:else}Сохранить вопрос{/if}
				</Button>
				<Button variant="outline" onclick={oncancel} disabled={saving}>Отмена</Button>
			</div>
		</form>
	</Card.Content>
</Card.Root>
