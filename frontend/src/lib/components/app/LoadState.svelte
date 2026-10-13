<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import type { Resource } from '$lib/resource.svelte';

	/**
	 * Показывает загрузку и ошибку ресурса, а данные — через `children`.
	 * 404 показывается как «нет доступа или не найдено» (сервер не различает).
	 */
	let { resource, children }: { resource: Resource<T>; children: Snippet<[T]> } = $props();
</script>

{#if resource.data !== null}
	{@render children(resource.data)}
{:else if resource.error}
	<Alert.Root variant="destructive">
		<CircleAlertIcon />
		<Alert.Title>
			{resource.status === 404 ? 'Не найдено или нет доступа' : 'Не удалось загрузить'}
		</Alert.Title>
		<Alert.Description>
			<p>{resource.error}</p>
			{#if resource.status !== 404}
				<Button size="sm" variant="outline" class="mt-2" onclick={() => resource.reload()}>Повторить</Button>
			{/if}
		</Alert.Description>
	</Alert.Root>
{:else}
	<div class="text-muted-foreground flex items-center justify-center gap-2 py-16 text-sm" role="status">
		<LoaderIcon class="size-5 animate-spin" />
		Загрузка…
	</div>
{/if}
