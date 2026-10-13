<script lang="ts">
	import { page } from '$app/state';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import AuditLog from '$lib/components/audit/AuditLog.svelte';

	const courseId = $derived(page.params.id ?? '');
</script>

<svelte:head>
	<title>Журнал действий курса — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-5xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title="Журнал действий курса"
				description="Кто и когда менял курс, тесты, окна и решения по антифроду. Время — МСК."
				back={{ href: `/courses/${courseId}`, label: 'К курсу' }}
			/>
			{#key courseId}
				<AuditLog {courseId} />
			{/key}
		</div>
	{/snippet}
</RequireAuth>
