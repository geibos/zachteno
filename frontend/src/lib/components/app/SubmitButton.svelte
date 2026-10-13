<script lang="ts">
	import type { Snippet } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { Button, type ButtonVariant } from '$lib/components/ui/button';
	import { cn } from '$lib/utils';

	let {
		pending = false,
		pendingText,
		variant = 'default',
		class: className = '',
		children
	}: {
		pending?: boolean;
		pendingText?: string;
		variant?: ButtonVariant;
		class?: string;
		children: Snippet;
	} = $props();
</script>

<Button type="submit" {variant} size="lg" class={cn('w-full', className)} disabled={pending} aria-busy={pending}>
	{#if pending}
		<LoaderIcon class="animate-spin" />
		{pendingText ?? 'Подождите…'}
	{:else}
		{@render children()}
	{/if}
</Button>
