<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import type { SaveStatus } from '$lib/attempt/save-queue';

	let { status, message }: { status: SaveStatus | null; message?: string | null } = $props();
</script>

<span class="inline-flex items-center gap-1 text-xs" role="status" aria-live="polite">
	{#if status === 'saved'}
		<CheckIcon class="size-3.5 text-emerald-600 dark:text-emerald-400" />
		<span class="text-muted-foreground">Сохранено</span>
	{:else if status === 'saving'}
		<LoaderIcon class="text-muted-foreground size-3.5 animate-spin" />
		<span class="text-muted-foreground">Сохраняется…</span>
	{:else if status === 'unsaved'}
		<CloudOffIcon class="size-3.5 text-amber-600 dark:text-amber-400" />
		<span class="text-amber-700 dark:text-amber-300">Не сохранено</span>
	{:else if status === 'error'}
		<CircleAlertIcon class="text-destructive size-3.5" />
		<span class="text-destructive">{message ?? 'Не удалось сохранить'}</span>
	{/if}
</span>
