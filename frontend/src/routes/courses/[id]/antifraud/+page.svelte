<script lang="ts">
	import { page } from '$app/state';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import AntifraudView from '$lib/components/antifraud/AntifraudView.svelte';

	const courseId = $derived(page.params.id ?? '');
</script>

<svelte:head>
	<title>Связанные аккаунты — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		{#key courseId}
			<AntifraudView {courseId} />
		{/key}
	{/snippet}
</RequireAuth>
