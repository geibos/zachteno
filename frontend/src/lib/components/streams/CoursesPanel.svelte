<script lang="ts">
	import BookOpenIcon from '@lucide/svelte/icons/book-open';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import { toast } from 'svelte-sonner';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import * as Select from '$lib/components/ui/select';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { resource } from '$lib/resource.svelte';

	type StreamCourse = Schema<'StreamCourse'>;

	let {
		streamId,
		courses,
		onchange
	}: { streamId: string; courses: StreamCourse[]; onchange: () => void } = $props();

	// Курсы, которые я вижу; назначить можно только неархивные и ещё не назначенные.
	const myCourses = resource(() => unwrap(api.GET('/api/courses')));
	const available = $derived(
		(myCourses.data ?? []).filter(
			(course) => !course.archived && !courses.some((assigned) => assigned.course_id === course.id)
		)
	);

	let selected = $state('');
	let assigning = $state(false);
	const selectedTitle = $derived(available.find((course) => course.id === selected)?.title);

	async function assign(): Promise<void> {
		if (!selected) return;
		assigning = true;
		try {
			await unwrap(
				api.POST('/api/streams/{stream_id}/courses', {
					params: { path: { stream_id: streamId } },
					body: { course_id: selected }
				})
			);
			toast.success(`Курс «${selectedTitle ?? ''}» назначен потоку`);
			selected = '';
			onchange();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			assigning = false;
		}
	}

	let target = $state<StreamCourse | null>(null);
	let confirmOpen = $state(false);

	async function unassign(): Promise<void> {
		if (!target) return;
		const course = target;
		await unwrap(
			api.DELETE('/api/streams/{stream_id}/courses/{course_id}', {
				params: { path: { stream_id: streamId, course_id: course.course_id } }
			})
		);
		toast.success(`Курс «${course.title}» снят с потока`);
		onchange();
	}
</script>

<div class="grid gap-6">
	<div class="bg-card grid gap-2 rounded-xl border p-4">
		<Label for="assign-course">Назначить курс</Label>
		{#if myCourses.error}
			<p class="text-destructive text-sm">{myCourses.error}</p>
		{:else if myCourses.data === null}
			<p class="text-muted-foreground text-sm">Загружаем ваши курсы…</p>
		{:else if available.length === 0}
			<p class="text-muted-foreground text-sm">
				Нет курсов, которые можно назначить: все ваши курсы уже в потоке или в архиве.
			</p>
		{:else}
			<div class="flex flex-col gap-2 sm:flex-row">
				<Select.Root type="single" bind:value={selected}>
					<Select.Trigger id="assign-course" class="w-full min-w-0 sm:max-w-sm">
						<span class="truncate">{selectedTitle ?? 'Выберите курс'}</span>
					</Select.Trigger>
					<Select.Content>
						{#each available as course (course.id)}
							<Select.Item value={course.id} label={course.title}>{course.title}</Select.Item>
						{/each}
					</Select.Content>
				</Select.Root>
				<Button onclick={assign} disabled={!selected || assigning}>
					{#if assigning}<LoaderIcon class="animate-spin" />{:else}<PlusIcon />{/if}
					Назначить
				</Button>
			</div>
			<p class="text-muted-foreground text-xs">Студенты потока увидят тесты курса, когда для потока откроют окно прохождения.</p>
		{/if}
	</div>

	{#if courses.length === 0}
		<EmptyState icon={BookOpenIcon} title="Курсы не назначены" text="Назначьте потоку курс, чтобы студенты могли проходить его тесты." />
	{:else}
		<ul class="bg-card divide-y rounded-xl border">
			{#each courses as course (course.course_id)}
				<li class="flex flex-wrap items-center gap-3 p-4">
					<a href="/courses/{course.course_id}" class="text-primary min-w-0 flex-1 font-medium break-words underline-offset-3 hover:underline">
						{course.title}
					</a>
					<Button
						size="sm"
						variant="outline"
						onclick={() => {
							target = course;
							confirmOpen = true;
						}}
					>
						Снять
					</Button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<ConfirmDialog
	bind:open={confirmOpen}
	title="Снять курс с потока?"
	description={target
		? `Студенты потока перестанут видеть тесты курса «${target.title}». Сданные попытки и результаты сохранятся.`
		: undefined}
	confirmLabel="Снять"
	destructive
	onconfirm={unassign}
/>
