<script lang="ts">
	import { page } from '$app/state';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import TestEditor from '$lib/components/editor/TestEditor.svelte';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';

	const testId = $derived(page.params.id ?? '');
	const test = new Resource(() =>
		unwrap(api.GET('/api/tests/{test_id}', { params: { path: { test_id: testId } } }))
	);

	// Новый id (например, после копирования) — загрузить заново.
	$effect(() => {
		if (testId) {
			test.data = null;
			void test.reload();
		}
	});
</script>

<svelte:head>
	<title>{test.data ? `${test.data.title} — редактор` : 'Тест'} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		{#if test.data}
			<TestEditor test={test.data} reload={() => test.reload()} />
		{:else}
			<div class="mx-auto w-full max-w-5xl px-4 py-8 sm:px-6">
				<LoadState resource={test}>
					{#snippet children()}{/snippet}
				</LoadState>
			</div>
		{/if}
	{/snippet}
</RequireAuth>
