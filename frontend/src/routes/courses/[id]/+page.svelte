<script lang="ts">
	import { page } from '$app/state';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import CourseView from './CourseView.svelte';

	const courseId = $derived(page.params.id ?? '');
	const course = new Resource(() =>
		unwrap(api.GET('/api/courses/{course_id}', { params: { path: { course_id: courseId } } }))
	);

	$effect(() => {
		if (courseId) {
			course.data = null;
			void course.reload();
		}
	});
</script>

<svelte:head>
	<title>{course.data?.title ?? 'Курс'} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
			<LoadState resource={course}>
				{#snippet children(data)}
					<CourseView course={data} reload={() => course.reload()} />
				{/snippet}
			</LoadState>
		</div>
	{/snippet}
</RequireAuth>
