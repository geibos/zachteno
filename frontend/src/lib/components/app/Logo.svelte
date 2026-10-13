<script lang="ts">
	import { cn } from '$lib/utils';

	let { class: className = '', withText = true }: { class?: string; withText?: boolean } = $props();
</script>

<span class={cn('inline-flex items-center gap-2 font-semibold tracking-tight', className)}>
	<svg viewBox="0 0 32 32" class="size-7 shrink-0" aria-hidden="true">
		<rect width="32" height="32" rx="8" class="fill-primary" />
		<path
			d="M9 16.5l4.5 4.5L23 11.5"
			fill="none"
			class="stroke-primary-foreground"
			stroke-width="3.2"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	</svg>
	{#if withText}
		<span class="text-lg">Зачтено</span>
	{/if}
</span>
