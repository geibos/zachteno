<script lang="ts">
	import { Textarea } from '$lib/components/ui/textarea';
	import { Label } from '$lib/components/ui/label';
	import Markdown from '$lib/components/app/Markdown.svelte';

	/** Текст в Markdown с формулами и живым предпросмотром. */
	let {
		id,
		label,
		value = $bindable(''),
		rows = 3,
		placeholder,
		hideLabel = false
	}: {
		id: string;
		label: string;
		value?: string;
		rows?: number;
		placeholder?: string;
		hideLabel?: boolean;
	} = $props();
</script>

<div class="grid gap-2">
	<Label for={id} class={hideLabel ? 'sr-only' : undefined}>{label}</Label>
	<Textarea {id} bind:value {rows} {placeholder} class="font-mono text-sm" />
	{#if value.trim() !== ''}
		<div class="bg-muted/40 rounded-md border px-3 py-2">
			<div class="text-muted-foreground mb-1 text-[11px] font-medium tracking-wide uppercase">
				Предпросмотр
			</div>
			<Markdown source={value} />
		</div>
	{/if}
</div>
