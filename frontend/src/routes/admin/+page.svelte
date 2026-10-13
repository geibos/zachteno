<script lang="ts">
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import UsersPanel from '$lib/components/admin/UsersPanel.svelte';
	import TrustedLinksPanel from '$lib/components/admin/TrustedLinksPanel.svelte';

	let tab = $state('users');
</script>

<svelte:head>
	<title>Администрирование — Зачтено</title>
</svelte:head>

<RequireAuth role="superuser">
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title="Администрирование"
				description="Пользователи и их роли, блокировка, ссылки доверенной регистрации."
			/>
			<Tabs.Root bind:value={tab} class="gap-4">
				<div class="-mx-4 overflow-x-auto px-4 sm:mx-0 sm:px-0">
					<Tabs.List>
						<Tabs.Trigger value="users">Пользователи</Tabs.Trigger>
						<Tabs.Trigger value="trusted">Доверенные ссылки</Tabs.Trigger>
					</Tabs.List>
				</div>
				<Tabs.Content value="users"><UsersPanel /></Tabs.Content>
				<Tabs.Content value="trusted"><TrustedLinksPanel /></Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</RequireAuth>
