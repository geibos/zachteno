<script lang="ts">
	import { goto } from '$app/navigation';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import BookOpenIcon from '@lucide/svelte/icons/book-open';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { count } from '$lib/format';
	import { resource } from '$lib/resource.svelte';

	const courses = resource(() => unwrap(api.GET('/api/courses')));

	let showArchived = $state(false);
	let mineOnly = $state(false);

	const canCreate = $derived(!!session.user?.roles.methodist);

	let createOpen = $state(false);
	let title = $state('');
	let description = $state('');
	let creating = $state(false);
	let createError = $state<string | null>(null);

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		createError = null;
		if (title.trim() === '') {
			createError = 'Укажите название курса.';
			return;
		}
		creating = true;
		try {
			const created = await unwrap(
				api.POST('/api/courses', { body: { title: title.trim(), description: description.trim() } })
			);
			toast.success('Курс создан');
			createOpen = false;
			await goto(`/courses/${created.id}`);
		} catch (error) {
			createError = errorMessage(error);
		} finally {
			creating = false;
		}
	}
</script>

<svelte:head>
	<title>Курсы — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children(user)}
		<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title="Курсы"
				description={user.roles.methodist || user.roles.superuser
					? 'Все курсы: свои можно редактировать, чужие — смотреть и копировать.'
					: 'Курсы, на которые вы назначены преподавателем.'}
			>
				{#snippet actions()}
					{#if canCreate}
						<Button
							onclick={() => {
								title = '';
								description = '';
								createError = null;
								createOpen = true;
							}}
						>
							<PlusIcon />
							Создать курс
						</Button>
					{/if}
				{/snippet}
			</PageHeader>

			<LoadState resource={courses}>
				{#snippet children(list)}
					{@const visible = list.filter(
						(c) => (showArchived || !c.archived) && (!mineOnly || c.owner_id === user.id)
					)}
					<div class="flex flex-wrap gap-x-6 gap-y-2 text-sm">
						{#if user.roles.methodist}
							<label class="flex items-center gap-2"><Checkbox bind:checked={mineOnly} />Только мои</label>
						{/if}
						<label class="flex items-center gap-2"><Checkbox bind:checked={showArchived} />Показывать архивные</label>
					</div>
					{#if visible.length === 0}
						<EmptyState
							icon={BookOpenIcon}
							title="Курсов нет"
							text={canCreate
								? 'Создайте первый курс — в нём будут тесты.'
								: 'Когда методист назначит вас преподавателем, курс появится здесь.'}
						/>
					{:else}
						<div class="grid gap-4 sm:grid-cols-2">
							{#each visible as course (course.id)}
								<a href="/courses/{course.id}" class="group focus-visible:ring-ring/50 rounded-xl outline-none focus-visible:ring-3">
									<Card.Root class="group-hover:border-primary/40 h-full transition-colors">
										<Card.Header>
											<Card.Title class="flex flex-wrap items-center gap-2 break-words">
												{course.title}
												{#if course.archived}<Badge variant="outline">в архиве</Badge>{/if}
												{#if course.owner_id === user.id}<Badge variant="secondary">мой</Badge>{/if}
											</Card.Title>
											{#if course.description}
												<Card.Description class="line-clamp-2">{course.description}</Card.Description>
											{/if}
										</Card.Header>
										<Card.Content class="text-muted-foreground text-sm">
											{count(course.tests_count, ['тест', 'теста', 'тестов'])} · владелец {course.owner_name}
										</Card.Content>
									</Card.Root>
								</a>
							{/each}
						</div>
					{/if}
				{/snippet}
			</LoadState>
		</div>

		<Dialog.Root bind:open={createOpen}>
			<Dialog.Content>
				<form class="grid gap-4" novalidate onsubmit={create}>
					<Dialog.Header>
						<Dialog.Title>Новый курс</Dialog.Title>
						<Dialog.Description>Вы станете владельцем курса.</Dialog.Description>
					</Dialog.Header>
					<div class="grid gap-2">
						<Label for="course-title">Название</Label>
						<Input id="course-title" bind:value={title} maxlength={300} />
					</div>
					<div class="grid gap-2">
						<Label for="course-description">Описание</Label>
						<Textarea id="course-description" bind:value={description} rows={3} />
					</div>
					{#if createError}<p class="text-destructive text-sm" aria-live="polite">{createError}</p>{/if}
					<Dialog.Footer>
						<Button variant="outline" onclick={() => (createOpen = false)} disabled={creating}>Отмена</Button>
						<Button type="submit" disabled={creating}>
							{#if creating}<LoaderIcon class="animate-spin" />{/if}
							Создать
						</Button>
					</Dialog.Footer>
				</form>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</RequireAuth>
