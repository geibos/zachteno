<script lang="ts">
	import ComingSoon from '$lib/components/app/ComingSoon.svelte';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
</script>

<svelte:head>
	<title>Администрирование — Зачтено</title>
</svelte:head>

<RequireAuth role="superuser">
	{#snippet children()}
		<ComingSoon
			title="Администрирование"
			text="Здесь будут пользователи и роли, ссылки доверенной регистрации и аудит-лог."
		/>
	{/snippet}
</RequireAuth>
