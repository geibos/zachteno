<script lang="ts">
	import type { Snippet } from 'svelte';
	import CheckIcon from '@lucide/svelte/icons/check';
	import PinIcon from '@lucide/svelte/icons/pin';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import Markdown from '$lib/components/app/Markdown.svelte';
	import type { Schema } from '$lib/api/client';
	import { formatScore } from '$lib/format';
	import { KIND_SHORT } from './question-model';

	let {
		question,
		number,
		actions
	}: { question: Schema<'QuestionDetail'>; number: number; actions?: Snippet } = $props();

	const hasPenalty = (penalty: string) => penalty !== '' && Number(penalty) !== 0;
</script>

<Card.Root>
	<Card.Header class="gap-2">
		<div class="flex flex-wrap items-start justify-between gap-2">
			<div class="flex flex-wrap items-center gap-2">
				<span class="font-semibold">Вопрос {number}</span>
				<Badge variant="secondary">{KIND_SHORT[question.kind]}</Badge>
				<Badge variant="outline">вес {formatScore(question.weight)}</Badge>
			</div>
			{#if actions}<div class="flex flex-wrap items-center gap-1">{@render actions()}</div>{/if}
		</div>
	</Card.Header>
	<Card.Content class="grid gap-3">
		{#if question.body.trim()}
			<Markdown source={question.body} />
		{/if}
		{#if question.image}
			<img
				src={question.image.url}
				alt={question.image.alt}
				class="max-h-64 w-fit max-w-full rounded-md border object-contain"
				loading="lazy"
			/>
		{/if}
		{#if question.kind === 'free_text'}
			<p class="text-muted-foreground text-sm">Свободный ответ — не оценивается.</p>
		{:else}
			<ul class="grid gap-2">
				{#each question.options as option (option.id)}
					<li
						class="flex items-start gap-2 rounded-md border px-3 py-2 text-sm {option.is_correct
							? 'border-emerald-500/50 bg-emerald-50 dark:bg-emerald-950/30'
							: ''}"
					>
						<span class="mt-0.5 flex size-4 shrink-0 items-center justify-center">
							{#if option.is_correct}
								<CheckIcon class="size-4 text-emerald-600 dark:text-emerald-400" aria-label="верный" />
							{/if}
						</span>
						<div class="grid min-w-0 flex-1 gap-2">
							{#if option.body.trim()}<Markdown source={option.body} />{/if}
							{#if option.image}
								<img
									src={option.image.url}
									alt={option.image.alt}
									class="max-h-40 w-fit max-w-full rounded border object-contain"
									loading="lazy"
								/>
							{/if}
						</div>
						<div class="flex shrink-0 items-center gap-1">
							{#if option.pinned}<PinIcon class="text-muted-foreground size-3.5" aria-label="закреплён" />{/if}
							{#if hasPenalty(option.penalty)}
								<Badge variant="outline">штраф {formatScore(option.penalty)}</Badge>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</Card.Content>
</Card.Root>
