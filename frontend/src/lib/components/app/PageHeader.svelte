<script lang="ts">
	import type { Snippet } from 'svelte';
	import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';

	let {
		title,
		description,
		back,
		badges,
		actions
	}: {
		title: string;
		description?: string;
		back?: { href: string; label: string };
		badges?: Snippet;
		actions?: Snippet;
	} = $props();
</script>

<div class="grid gap-3">
	{#if back}
		<a
			href={back.href}
			class="text-muted-foreground hover:text-foreground inline-flex w-fit items-center gap-1 text-sm"
		>
			<ArrowLeftIcon class="size-4" />
			{back.label}
		</a>
	{/if}
	<div class="flex flex-wrap items-start justify-between gap-3">
		<div class="min-w-0">
			<h1 class="text-2xl font-semibold tracking-tight break-words">{title}</h1>
			{#if badges}
				<div class="mt-2 flex flex-wrap items-center gap-2">{@render badges()}</div>
			{/if}
			{#if description}
				<p class="text-muted-foreground mt-1 text-sm">{description}</p>
			{/if}
		</div>
		{#if actions}
			<div class="flex flex-wrap items-center gap-2">{@render actions()}</div>
		{/if}
	</div>
</div>
