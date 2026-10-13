<script lang="ts">
	import type { Component, Snippet } from 'svelte';
	import InboxIcon from '@lucide/svelte/icons/inbox';

	let {
		title,
		text,
		icon: Icon = InboxIcon,
		children
	}: { title: string; text?: string; icon?: Component<{ class?: string }>; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center gap-2 rounded-lg border border-dashed px-4 py-10 text-center">
	<Icon class="text-muted-foreground size-8" />
	<p class="font-medium">{title}</p>
	{#if text}<p class="text-muted-foreground max-w-sm text-sm">{text}</p>{/if}
	{#if children}<div class="mt-2">{@render children()}</div>{/if}
</div>
