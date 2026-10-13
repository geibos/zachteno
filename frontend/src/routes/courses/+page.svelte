<script lang="ts">
	import ComingSoon from '$lib/components/app/ComingSoon.svelte';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
</script>

<svelte:head>
	<title>Курсы — Зачтено</title>
</svelte:head>

<RequireAuth role="methodist">
	{#snippet children()}
		<ComingSoon title="Курсы" text="Здесь методист будет создавать курсы и тесты, а также назначать преподавателей." />
	{/snippet}
</RequireAuth>
