<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { selectClass } from './styles';

	/** Копия теста в выбранный свой курс; клон — редактируемый черновик без попыток. */
	let {
		open = $bindable(false),
		testId,
		currentCourseId
	}: { open?: boolean; testId: string; currentCourseId: string } = $props();

	let courses = $state<Schema<'CourseSummary'>[] | null>(null);
	let target = $state('');
	let loadError = $state<string | null>(null);
	let saving = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (open && courses === null) void load();
	});

	async function load(): Promise<void> {
		loadError = null;
		try {
			const all = await unwrap(api.GET('/api/courses'));
			const user = session.user;
			// Копировать можно только в свои курсы (суперпользователю — в любые).
			courses = all.filter(
				(course) => !course.archived && (user?.roles.superuser || course.owner_id === user?.id)
			);
			target = courses.some((c) => c.id === currentCourseId) ? currentCourseId : (courses[0]?.id ?? '');
		} catch (err) {
			loadError = errorMessage(err);
		}
	}

	async function clone(): Promise<void> {
		if (!target) return;
		saving = true;
		error = null;
		try {
			const created = await unwrap(
				api.POST('/api/tests/{test_id}/clone', {
					params: { path: { test_id: testId } },
					body: { course_id: target }
				})
			);
			open = false;
			toast.success('Копия создана — это черновик, его можно редактировать');
			await goto(`/tests/${created.id}`);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			saving = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Копировать тест</Dialog.Title>
			<Dialog.Description>
				Копия — черновик с теми же вопросами и настройками, без попыток и без блокировки.
			</Dialog.Description>
		</Dialog.Header>
		{#if loadError}
			<p class="text-destructive text-sm">{loadError}</p>
		{:else if courses === null}
			<p class="text-muted-foreground flex items-center gap-2 text-sm">
				<LoaderIcon class="size-4 animate-spin" />Загружаем курсы…
			</p>
		{:else if courses.length === 0}
			<p class="text-sm">Нет ваших курсов, в которые можно скопировать тест. Сначала создайте курс.</p>
		{:else}
			<div class="grid gap-2">
				<Label for="clone-target">В курс</Label>
				<select id="clone-target" class={selectClass} bind:value={target}>
					{#each courses as course (course.id)}
						<option value={course.id}>{course.title}</option>
					{/each}
				</select>
			</div>
		{/if}
		{#if error}<p class="text-destructive text-sm" aria-live="polite">{error}</p>{/if}
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)} disabled={saving}>Отмена</Button>
			<Button onclick={clone} disabled={saving || !target}>
				{#if saving}<LoaderIcon class="animate-spin" />{/if}
				Создать копию
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
