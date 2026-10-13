<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import AttemptRunner from '$lib/components/runner/AttemptRunner.svelte';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';

	const attemptId = $derived(page.params.id ?? '');

	const attempt = new Resource(() =>
		unwrap(api.GET('/api/attempts/{attempt_id}', { params: { path: { attempt_id: attemptId } } }))
	);

	$effect(() => {
		void attemptId;
		void attempt.reload();
	});

	// Сданную попытку открываем на разборе.
	$effect(() => {
		if (attempt.data?.submitted_at) void goto(`/attempts/${attemptId}/review`, { replaceState: true });
	});
</script>

<svelte:head>
	<title>{attempt.data?.title ?? 'Попытка'} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<LoadState resource={attempt}>
			{#snippet children(data)}
				{#if !data.submitted_at}
					{#key data.id}
						<AttemptRunner initial={data} />
					{/key}
				{/if}
			{/snippet}
		</LoadState>
	{/snippet}
</RequireAuth>
