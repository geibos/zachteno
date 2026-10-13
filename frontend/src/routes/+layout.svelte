<script lang="ts">
	import '../app.css';
	import { onMount, type Snippet } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
	import AppHeader from '$lib/components/app/AppHeader.svelte';
	import { Toaster } from '$lib/components/ui/sonner';
	import { Button } from '$lib/components/ui/button';
	import * as Alert from '$lib/components/ui/alert';
	import { session } from '$lib/api/session.svelte';

	let { children }: { children: Snippet } = $props();

	onMount(() => {
		void session.load();
	});
</script>

<div class="flex min-h-dvh flex-col">
	<a
		href="#main"
		class="bg-primary text-primary-foreground sr-only z-50 rounded-md px-3 py-2 focus:not-sr-only focus:fixed focus:top-2 focus:left-2"
	>
		К содержимому
	</a>

	<AppHeader />

	<main id="main" class="flex flex-1 flex-col">
		{#if session.ready}
			{@render children()}
		{:else if session.status === 'error'}
			<div class="mx-auto w-full max-w-md px-4 py-14">
				<Alert.Root variant="destructive">
					<CloudOffIcon />
					<Alert.Title>Не удалось загрузить данные</Alert.Title>
					<Alert.Description>
						<p>{session.error}</p>
						<Button class="mt-3" variant="outline" size="sm" onclick={() => session.load()}>Повторить</Button>
					</Alert.Description>
				</Alert.Root>
			</div>
		{:else}
			<div class="text-muted-foreground flex flex-1 items-center justify-center gap-2 py-20 text-sm" role="status">
				<LoaderIcon class="size-5 animate-spin" />
				Загрузка…
			</div>
		{/if}
	</main>

	<footer class="text-muted-foreground border-t py-6 text-center text-xs">
		Зачтено · сервис тестирования студентов
	</footer>
</div>

<Toaster richColors position="top-center" closeButton />
