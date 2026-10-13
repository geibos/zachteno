<script lang="ts">
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import ProfileView from './ProfileView.svelte';
</script>

<svelte:head>
	<title>Профиль — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children(user)}
		<ProfileView {user} />
	{/snippet}
</RequireAuth>
