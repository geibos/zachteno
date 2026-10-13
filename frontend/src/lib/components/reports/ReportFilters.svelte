<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { selectClass } from '$lib/components/editor/styles';
	import { cn } from '$lib/utils';
	import type { Mode } from './table';

	let {
		streams,
		streamId = $bindable(''),
		mode = $bindable<Mode>('final'),
		csvHref,
		csvName
	}: {
		streams: Array<{ id: string; name: string }>;
		streamId?: string;
		mode?: Mode;
		csvHref?: string;
		csvName?: string;
	} = $props();

	const modes: Array<{ value: Mode; label: string }> = [
		{ value: 'final', label: 'Итоговая попытка' },
		{ value: 'all', label: 'Все попытки' }
	];
</script>

<div class="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-end">
	<div class="grid gap-1.5 sm:w-64">
		<Label for="report-stream">Поток</Label>
		<select id="report-stream" class={selectClass} bind:value={streamId}>
			<option value="">Все потоки</option>
			{#each streams as stream (stream.id)}
				<option value={stream.id}>{stream.name}</option>
			{/each}
		</select>
	</div>

	<div class="grid gap-1.5">
		<span class="text-sm font-medium" id="report-mode-label">Попытки</span>
		<div class="bg-muted inline-flex w-fit rounded-lg p-1" role="radiogroup" aria-labelledby="report-mode-label">
			{#each modes as item (item.value)}
				<button
					type="button"
					role="radio"
					aria-checked={mode === item.value}
					class={cn(
						'rounded-md px-3 py-1.5 text-sm font-medium transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
						mode === item.value ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'
					)}
					onclick={() => (mode = item.value)}
				>
					{item.label}
				</button>
			{/each}
		</div>
	</div>

	{#if csvHref}
		<Button href={csvHref} download={csvName} variant="outline" class="sm:ml-auto" data-sveltekit-reload>
			<DownloadIcon />Скачать CSV
		</Button>
	{/if}
</div>
