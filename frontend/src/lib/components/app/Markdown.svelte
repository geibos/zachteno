<script lang="ts">
	import { renderMarkdown, renderMarkdownInline } from '$lib/markdown';
	import { cn } from '$lib/utils';

	let {
		source,
		inline = false,
		class: className = ''
	}: { source: string; inline?: boolean; class?: string } = $props();

	// renderMarkdown экранирует сырой HTML (html: false), поэтому {@html} здесь безопасен.
	const html = $derived(inline ? renderMarkdownInline(source) : renderMarkdown(source));
</script>

{#if inline}
	<span class={cn('md-content', className)}>{@html html}</span>
{:else}
	<div class={cn('md-content', className)}>{@html html}</div>
{/if}
