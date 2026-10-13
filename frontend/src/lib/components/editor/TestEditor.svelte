<script lang="ts">
	import { goto } from '$app/navigation';
	import LockIcon from '@lucide/svelte/icons/lock';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import SendIcon from '@lucide/svelte/icons/send';
	import ArchiveIcon from '@lucide/svelte/icons/archive';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import ListChecksIcon from '@lucide/svelte/icons/list-checks';
	import { toast } from 'svelte-sonner';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import TestStatusBadge from '$lib/components/app/TestStatusBadge.svelte';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { count, formatDateTime } from '$lib/format';
	import CloneTestDialog from './CloneTestDialog.svelte';
	import QuestionEditor from './QuestionEditor.svelte';
	import QuestionView from './QuestionView.svelte';
	import SettingsForm from './SettingsForm.svelte';
	import WindowsPanel from './WindowsPanel.svelte';
	import { emptyDraft, fromDetail, toInput, type QuestionDraft } from './question-model';

	let { test, reload }: { test: Schema<'TestDetail'>; reload: () => Promise<void> } = $props();

	const locked = $derived(!!test.locked_at);
	const editableContent = $derived(test.can_edit && !locked);
	const questions = $derived([...test.questions].sort((a, b) => a.position - b.position));
	const canClone = $derived(!!session.user?.roles.methodist || !!session.user?.roles.superuser);

	// --- название и описание ---
	let title = $state('');
	let description = $state('');
	$effect.pre(() => {
		title = test.title;
		description = test.description;
	});
	const infoDirty = $derived(title.trim() !== test.title || description !== test.description);
	let infoSaving = $state(false);
	let infoError = $state<string | null>(null);

	async function saveInfo(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		infoError = null;
		if (title.trim() === '') {
			infoError = 'Укажите название теста.';
			return;
		}
		infoSaving = true;
		try {
			await unwrap(
				api.PATCH('/api/tests/{test_id}', {
					params: { path: { test_id: test.id } },
					body: { title: title.trim(), description }
				})
			);
			toast.success('Сохранено');
			await reload();
		} catch (error) {
			infoError = errorMessage(error);
		} finally {
			infoSaving = false;
		}
	}

	// --- действия с тестом ---
	let publishing = $state(false);
	let publishError = $state<string | null>(null);
	let archiveOpen = $state(false);
	let cloneOpen = $state(false);
	let previewing = $state(false);

	async function publish(): Promise<void> {
		publishing = true;
		publishError = null;
		try {
			await unwrap(api.POST('/api/tests/{test_id}/publish', { params: { path: { test_id: test.id } } }));
			toast.success('Тест опубликован');
			await reload();
		} catch (error) {
			publishError = errorMessage(error);
		} finally {
			publishing = false;
		}
	}

	async function preview(): Promise<void> {
		previewing = true;
		try {
			const started = await unwrap(
				api.POST('/api/tests/{test_id}/preview', { params: { path: { test_id: test.id } } })
			);
			await goto(`/attempts/${started.attempt_id}`);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			previewing = false;
		}
	}

	// --- вопросы ---
	/** `null` — ничего не редактируем, `'new'` — новый вопрос, иначе id вопроса. */
	let editing = $state<string | null>(null);
	let newDraft = $state<QuestionDraft>(emptyDraft());
	let deleting = $state<Schema<'QuestionDetail'> | null>(null);
	let deleteOpen = $state(false);
	let reordering = $state(false);

	function startNew(): void {
		newDraft = emptyDraft();
		editing = 'new';
	}

	async function createQuestion(draft: QuestionDraft): Promise<void> {
		await unwrap(
			api.POST('/api/tests/{test_id}/questions', {
				params: { path: { test_id: test.id } },
				body: toInput(draft)
			})
		);
		toast.success('Вопрос добавлен');
		editing = null;
		await reload();
	}

	async function updateQuestion(id: string, draft: QuestionDraft): Promise<void> {
		await unwrap(
			api.PUT('/api/questions/{question_id}', {
				params: { path: { question_id: id } },
				body: toInput(draft)
			})
		);
		toast.success('Вопрос сохранён');
		editing = null;
		await reload();
	}

	async function deleteQuestion(): Promise<void> {
		if (!deleting) return;
		await unwrap(
			api.DELETE('/api/questions/{question_id}', { params: { path: { question_id: deleting.id } } })
		);
		toast.success('Вопрос удалён');
		await reload();
	}

	async function move(index: number, delta: number): Promise<void> {
		const ids = questions.map((q) => q.id);
		const target = index + delta;
		if (target < 0 || target >= ids.length) return;
		const [id] = ids.splice(index, 1);
		if (id) ids.splice(target, 0, id);
		reordering = true;
		try {
			await unwrap(
				api.POST('/api/tests/{test_id}/questions/order', {
					params: { path: { test_id: test.id } },
					body: { question_ids: ids }
				})
			);
			await reload();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			reordering = false;
		}
	}
</script>

<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
	<PageHeader title={test.title} back={{ href: `/courses/${test.course_id}`, label: 'К курсу' }}>
		{#snippet badges()}
			<TestStatusBadge status={test.status} />
			<span class="text-muted-foreground text-sm">
				{count(questions.length, ['вопрос', 'вопроса', 'вопросов'])}
			</span>
			{#if !test.can_edit}<span class="text-muted-foreground text-sm">· только просмотр</span>{/if}
		{/snippet}
		{#snippet actions()}
			<Button variant="outline" onclick={preview} disabled={previewing || questions.length === 0}>
				{#if previewing}<LoaderIcon class="animate-spin" />{:else}<EyeIcon />{/if}
				Предпросмотр
			</Button>
			{#if canClone}
				<Button variant="outline" onclick={() => (cloneOpen = true)}>
					<CopyIcon />
					Копировать
				</Button>
			{/if}
			{#if test.can_edit && test.status !== 'archived'}
				<Button variant="outline" onclick={() => (archiveOpen = true)}>
					<ArchiveIcon />
					В архив
				</Button>
			{/if}
			{#if test.can_edit && test.status === 'draft'}
				<Button onclick={publish} disabled={publishing}>
					{#if publishing}<LoaderIcon class="animate-spin" />{:else}<SendIcon />{/if}
					Опубликовать
				</Button>
			{/if}
		{/snippet}
	</PageHeader>

	{#if publishError}
		<Alert.Root variant="destructive" aria-live="polite">
			<CircleAlertIcon />
			<Alert.Title>Опубликовать не получилось</Alert.Title>
			<Alert.Description>{publishError}</Alert.Description>
		</Alert.Root>
	{/if}

	{#if locked && test.locked_at}
		<Alert.Root class="border-amber-500/40 bg-amber-50 text-amber-950 dark:bg-amber-950/30 dark:text-amber-100">
			<LockIcon />
			<Alert.Title>Заблокирован с {formatDateTime(test.locked_at)}</Alert.Title>
			<Alert.Description class="text-current/80">
				<p>
					Уже есть ответы студентов. Менять можно название, описание и политику показа результатов.
					Чтобы изменить вопросы или настройки, сделайте копию.
				</p>
				{#if canClone}
					<Button size="sm" variant="outline" class="mt-2" onclick={() => (cloneOpen = true)}>
						<CopyIcon />
						Копировать тест
					</Button>
				{/if}
			</Alert.Description>
		</Alert.Root>
	{/if}

	{#if test.status === 'draft' && test.can_edit}
		<p class="text-muted-foreground text-sm">
			Черновик не виден студентам. Окно прохождения можно открыть после публикации.
		</p>
	{/if}

	<Card.Root>
		<Card.Header>
			<Card.Title>Основное</Card.Title>
		</Card.Header>
		<Card.Content>
			<form class="grid gap-4" novalidate onsubmit={saveInfo}>
				<div class="grid gap-2">
					<Label for="test-title">Название</Label>
					<Input id="test-title" bind:value={title} disabled={!test.can_edit} maxlength={300} />
				</div>
				<div class="grid gap-2">
					<Label for="test-description">Описание для студентов</Label>
					<Textarea
						id="test-description"
						bind:value={description}
						rows={3}
						disabled={!test.can_edit}
						placeholder="Что проверяет тест, чем можно пользоваться"
					/>
				</div>
				{#if infoError}<p class="text-destructive text-sm" aria-live="polite">{infoError}</p>{/if}
				{#if test.can_edit}
					<div>
						<Button type="submit" disabled={!infoDirty || infoSaving}>
							{#if infoSaving}<LoaderIcon class="animate-spin" />{/if}
							Сохранить
						</Button>
					</div>
				{/if}
			</form>
		</Card.Content>
	</Card.Root>

	<section class="grid gap-4" aria-labelledby="questions-heading">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h2 id="questions-heading" class="text-lg font-semibold">Вопросы</h2>
			{#if editableContent && editing !== 'new'}
				<Button size="sm" onclick={startNew}>
					<PlusIcon />
					Добавить вопрос
				</Button>
			{/if}
		</div>

		{#if questions.length === 0 && editing !== 'new'}
			<EmptyState
				icon={ListChecksIcon}
				title="Вопросов пока нет"
				text={editableContent ? 'Добавьте первый вопрос — с текстом, формулами и картинками.' : undefined}
			/>
		{/if}

		{#each questions as question, index (question.id)}
			{#if editing === question.id}
				<QuestionEditor
					draft={fromDetail(question)}
					number={index + 1}
					onsave={(draft) => updateQuestion(question.id, draft)}
					oncancel={() => (editing = null)}
				/>
			{:else}
				<QuestionView {question} number={index + 1}>
					{#snippet actions()}
						{#if editableContent}
							<Button
								size="icon-sm"
								variant="ghost"
								aria-label="Поднять вопрос {index + 1}"
								disabled={index === 0 || reordering}
								onclick={() => move(index, -1)}
							>
								<ArrowUpIcon />
							</Button>
							<Button
								size="icon-sm"
								variant="ghost"
								aria-label="Опустить вопрос {index + 1}"
								disabled={index === questions.length - 1 || reordering}
								onclick={() => move(index, 1)}
							>
								<ArrowDownIcon />
							</Button>
							<Button
								size="sm"
								variant="outline"
								onclick={() => (editing = question.id)}
								disabled={editing !== null}
							>
								<PencilIcon />
								Изменить
							</Button>
							<Button
								size="icon-sm"
								variant="ghost"
								aria-label="Удалить вопрос {index + 1}"
								onclick={() => {
									deleting = question;
									deleteOpen = true;
								}}
							>
								<TrashIcon />
							</Button>
						{/if}
					{/snippet}
				</QuestionView>
			{/if}
		{/each}

		{#if editing === 'new'}
			<QuestionEditor
				draft={newDraft}
				number={null}
				onsave={createQuestion}
				oncancel={() => (editing = null)}
			/>
		{/if}
	</section>

	<SettingsForm
		testId={test.id}
		settings={test.settings}
		questionCount={questions.length}
		canEdit={test.can_edit}
		{locked}
		onsaved={reload}
	/>

	<WindowsPanel testId={test.id} published={test.status === 'published'} />
</div>

<ConfirmDialog
	bind:open={archiveOpen}
	title="Отправить тест в архив?"
	description="Архивный тест скрыт из рабочих списков, открыть для него окно нельзя. Результаты сохранятся."
	confirmLabel="В архив"
	onconfirm={async () => {
		await unwrap(api.POST('/api/tests/{test_id}/archive', { params: { path: { test_id: test.id } } }));
		toast.success('Тест в архиве');
		await reload();
	}}
/>

<ConfirmDialog
	bind:open={deleteOpen}
	title="Удалить вопрос?"
	description="Вопрос и его варианты будут удалены без возможности восстановления."
	confirmLabel="Удалить"
	destructive
	onconfirm={deleteQuestion}
/>

{#if canClone}
	<CloneTestDialog bind:open={cloneOpen} testId={test.id} currentCourseId={test.course_id} />
{/if}
