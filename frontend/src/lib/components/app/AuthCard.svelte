<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Card from '$lib/components/ui/card';

	let {
		title,
		description,
		children,
		footer
	}: { title: string; description?: string; children: Snippet; footer?: Snippet } = $props();
</script>

<div class="flex justify-center px-4 py-8 sm:py-14">
	<Card.Root class="w-full max-w-md">
		<Card.Header>
			<Card.Title class="text-xl">{title}</Card.Title>
			{#if description}
				<Card.Description>{description}</Card.Description>
			{/if}
		</Card.Header>
		<Card.Content>
			{@render children()}
		</Card.Content>
		{#if footer}
			<Card.Footer class="text-muted-foreground flex-col items-center gap-1 text-sm">
				{@render footer()}
			</Card.Footer>
		{/if}
	</Card.Root>
</div>
