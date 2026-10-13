<script lang="ts">
	import { goto } from '$app/navigation';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import ArchiveIcon from '@lucide/svelte/icons/archive';
	import ArchiveRestoreIcon from '@lucide/svelte/icons/archive-restore';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import LockIcon from '@lucide/svelte/icons/lock';
	import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ScrollTextIcon from '@lucide/svelte/icons/scroll-text';
	import ChartIcon from '@lucide/svelte/icons/chart-column';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Badge } from '$lib/components/ui/badge';
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
	import { count } from '$lib/format';
	import TeachersPanel from './TeachersPanel.svelte';

	let { course, reload }: { course: Schema<'CourseDetail'>; reload: () => Promise<void> } = $props();

	const tests = $derived([...course.tests].sort((a, b) => a.position - b.position));
	const canClone = $derived(!!session.user?.roles.methodist);

	// --- правка ---
	let editOpen = $state(false);
	let title = $state('');
	let description = $state('');
	let saving = $state(false);
	let editError = $state<string | null>(null);

	function startEdit(): void {
		title = course.title;
		description = course.description;
		editError = null;
		editOpen = true;
	}

	async function saveEdit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (title.trim() === '') {
			editError = 'Укажите название курса.';
			return;
		}
		saving = true;
		editError = null;
		try {
			await unwrap(
				api.PATCH('/api/courses/{course_id}', {
					params: { path: { course_id: course.id } },
					body: { title: title.trim(), description }
				})
			);
			toast.success('Курс сохранён');
			editOpen = false;
			await reload();
		} catch (error) {
			editError = errorMessage(error);
		} finally {
			saving = false;
		}
	}

	// --- архив ---
	let archiveOpen = $state(false);

	async function toggleArchive(): Promise<void> {
		await unwrap(
			api.PATCH('/api/courses/{course_id}', {
				params: { path: { course_id: course.id } },
				body: { archived: !course.archived }
			})
		);
		toast.success(course.archived ? 'Курс возвращён из архива' : 'Курс в архиве');
		await reload();
	}

	// --- новый тест ---
	let testOpen = $state(false);
	let testTitle = $state('');
	let creatingTest = $state(false);
	let testError = $state<string | null>(null);

	async function createTest(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (testTitle.trim() === '') {
			testError = 'Укажите название теста.';
			return;
		}
		creatingTest = true;
		testError = null;
		try {
			const created = await unwrap(
				api.POST('/api/courses/{course_id}/tests', {
					params: { path: { course_id: course.id } },
					body: { title: testTitle.trim() }
				})
			);
			testOpen = false;
			await goto(`/tests/${created.id}`);
		} catch (error) {
			testError = errorMessage(error);
		} finally {
			creatingTest = false;
		}
	}

	// --- копия курса ---
	let cloning = $state(false);

	async function cloneCourse(): Promise<void> {
		cloning = true;
		try {
			const created = await unwrap(
				api.POST('/api/courses/{course_id}/clone', { params: { path: { course_id: course.id } } })
			);
			toast.success('Копия курса создана — все тесты в ней черновики');
			await goto(`/courses/${created.id}`);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			cloning = false;
		}
	}
</script>

<PageHeader
	title={course.title}
	description={course.description || undefined}
	back={{ href: '/courses', label: 'Все курсы' }}
>
	{#snippet badges()}
		{#if course.archived}<Badge variant="outline">в архиве</Badge>{/if}
		<span class="text-muted-foreground text-sm">владелец {course.owner_name}</span>
		{#if !course.can_edit}<span class="text-muted-foreground text-sm">· только просмотр</span>{/if}
	{/snippet}
	{#snippet actions()}
		{#if canClone}
			<Button variant="outline" onclick={cloneCourse} disabled={cloning}>
				{#if cloning}<LoaderIcon class="animate-spin" />{:else}<CopyIcon />{/if}
				Копировать курс
			</Button>
		{/if}
		{#if course.can_edit}
			<Button variant="outline" onclick={startEdit}>
				<PencilIcon />
				Изменить
			</Button>
			<Button variant="outline" onclick={() => (archiveOpen = true)}>
				{#if course.archived}<ArchiveRestoreIcon />Из архива{:else}<ArchiveIcon />В архив{/if}
			</Button>
		{/if}
	{/snippet}
</PageHeader>

<nav aria-label="Отчёты курса" class="flex flex-wrap gap-2">
	<Button href="/courses/{course.id}/summary" variant="secondary" size="sm"><TableIcon />Сводная таблица</Button>
	<Button href="/courses/{course.id}/antifraud" variant="secondary" size="sm"><ShieldAlertIcon />Связанные аккаунты</Button>
	{#if course.can_edit}
		<Button href="/courses/{course.id}/audit" variant="secondary" size="sm"><ScrollTextIcon />Журнал действий</Button>
	{/if}
</nav>

<Card.Root>
	<Card.Header>
		<div class="flex flex-wrap items-center justify-between gap-2">
			<Card.Title>Тесты</Card.Title>
			{#if course.can_edit}
				<Button
					size="sm"
					onclick={() => {
						testTitle = '';
						testError = null;
						testOpen = true;
					}}
				>
					<PlusIcon />
					Создать тест
				</Button>
			{/if}
		</div>
	</Card.Header>
	<Card.Content>
		{#if tests.length === 0}
			<EmptyState
				icon={ClipboardListIcon}
				title="Тестов пока нет"
				text={course.can_edit ? 'Создайте тест и добавьте в него вопросы.' : undefined}
			/>
		{:else}
			<ul class="divide-y rounded-lg border">
				{#each tests as test (test.id)}
					<li>
						<a
							href="/tests/{test.id}"
							class="hover:bg-muted/50 flex flex-col gap-1 px-4 py-3 sm:flex-row sm:items-center sm:gap-3"
						>
							<span class="min-w-0 flex-1 font-medium break-words">{test.title}</span>
							<span class="flex flex-wrap items-center gap-2">
								<TestStatusBadge status={test.status} />
								{#if test.locked}
									<Badge variant="outline"><LockIcon />заблокирован</Badge>
								{/if}
								<span class="text-muted-foreground text-xs">
									{count(test.question_count, ['вопрос', 'вопроса', 'вопросов'])}
								</span>
							</span>
						</a>
					</li>
				{/each}
			</ul>
		{/if}
	</Card.Content>
</Card.Root>

<TeachersPanel courseId={course.id} canEdit={course.can_edit} />

<Dialog.Root bind:open={editOpen}>
	<Dialog.Content>
		<form class="grid gap-4" novalidate onsubmit={saveEdit}>
			<Dialog.Header>
				<Dialog.Title>Курс</Dialog.Title>
			</Dialog.Header>
			<div class="grid gap-2">
				<Label for="edit-title">Название</Label>
				<Input id="edit-title" bind:value={title} maxlength={300} />
			</div>
			<div class="grid gap-2">
				<Label for="edit-description">Описание</Label>
				<Textarea id="edit-description" bind:value={description} rows={4} />
			</div>
			{#if editError}<p class="text-destructive text-sm" aria-live="polite">{editError}</p>{/if}
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (editOpen = false)} disabled={saving}>Отмена</Button>
				<Button type="submit" disabled={saving}>
					{#if saving}<LoaderIcon class="animate-spin" />{/if}
					Сохранить
				</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={testOpen}>
	<Dialog.Content>
		<form class="grid gap-4" novalidate onsubmit={createTest}>
			<Dialog.Header>
				<Dialog.Title>Новый тест</Dialog.Title>
				<Dialog.Description>
					Тест создаётся черновиком с настройками по умолчанию: одна попытка, без лимита времени,
					результаты сразу после сдачи.
				</Dialog.Description>
			</Dialog.Header>
			<div class="grid gap-2">
				<Label for="test-title-new">Название</Label>
				<Input id="test-title-new" bind:value={testTitle} maxlength={300} />
			</div>
			{#if testError}<p class="text-destructive text-sm" aria-live="polite">{testError}</p>{/if}
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (testOpen = false)} disabled={creatingTest}>Отмена</Button>
				<Button type="submit" disabled={creatingTest}>
					{#if creatingTest}<LoaderIcon class="animate-spin" />{/if}
					Создать и открыть
				</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>

<ConfirmDialog
	bind:open={archiveOpen}
	title={course.archived ? 'Вернуть курс из архива?' : 'Отправить курс в архив?'}
	description={course.archived
		? 'Курс снова появится в рабочих списках.'
		: 'Архивный курс скрыт из рабочих списков, но его результаты сохраняются и доступны.'}
	confirmLabel={course.archived ? 'Вернуть' : 'В архив'}
	onconfirm={toggleArchive}
/>
