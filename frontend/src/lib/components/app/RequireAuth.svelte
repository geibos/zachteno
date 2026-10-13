<script lang="ts">
	import type { Snippet } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import * as Alert from '$lib/components/ui/alert';
	import { session } from '$lib/api/session.svelte';
	import type { CurrentUser } from '$lib/api/client';

	type Role = keyof CurrentUser['roles'];

	let { role, children }: { role?: Role; children: Snippet<[CurrentUser]> } = $props();

	$effect(() => {
		if (session.ready && !session.user && !session.leaving) {
			const next = page.url.pathname + page.url.search;
			void goto(`/login?next=${encodeURIComponent(next)}`, { replaceState: true });
		}
	});
</script>

{#if session.user}
	{#if role && !session.user.roles[role]}
		<div class="mx-auto w-full max-w-5xl px-4 py-10 sm:px-6">
			<Alert.Root variant="destructive">
				<ShieldAlertIcon />
				<Alert.Title>Нет доступа</Alert.Title>
				<Alert.Description>У вашей учётной записи нет прав на этот раздел.</Alert.Description>
			</Alert.Root>
		</div>
	{:else}
		{@render children(session.user)}
	{/if}
{/if}
