<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Badge } from '$lib/components/ui/badge';
	import Markdown from '$lib/components/app/Markdown.svelte';
	import SaveIndicator from './SaveIndicator.svelte';
	import type { Schema } from '$lib/api/client';
	import type { SaveStatus } from '$lib/attempt/save-queue';
	import type { LocalAnswer } from '$lib/attempt/answers';

	let {
		question,
		number,
		total,
		answer,
		status,
		statusMessage,
		disabled = false,
		onchange,
		ontextblur,
		footer
	}: {
		question: Schema<'RunnerQuestion'>;
		number: number;
		total: number;
		answer: LocalAnswer;
		status: SaveStatus | null;
		statusMessage?: string | null;
		disabled?: boolean;
		/** `debounce` — для набора текста. */
		onchange: (answer: LocalAnswer, debounce: boolean) => void;
		ontextblur?: () => void;
		footer?: Snippet;
	} = $props();

	const hint = $derived(
		question.choice === 'single'
			? 'Выберите один вариант'
			: question.choice === 'multiple'
				? 'Выберите все подходящие варианты'
				: 'Введите ответ'
	);

	function toggle(optionId: string, checked: boolean): void {
		const selected = checked
			? [...answer.selected.filter((id) => id !== optionId), optionId]
			: answer.selected.filter((id) => id !== optionId);
		onchange({ selected, text: answer.text }, false);
	}

	const optionClass =
		'flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors hover:bg-muted/50 has-[[data-checked]]:border-primary has-[[data-checked]]:bg-primary/5 has-[:disabled]:cursor-default';
</script>

<section class="grid gap-4" aria-labelledby="q-{question.id}-title">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h2 id="q-{question.id}-title" class="text-muted-foreground text-sm font-medium">
			Вопрос {number} из {total}
		</h2>
		<SaveIndicator {status} message={statusMessage} />
	</div>

	<Markdown source={question.body} class="text-base" />

	{#if question.image}
		<img
			src={question.image.url}
			alt={question.image.alt}
			class="max-h-96 w-auto max-w-full rounded-lg border object-contain"
			loading="lazy"
		/>
	{/if}

	<p class="text-muted-foreground text-xs">{hint}</p>

	{#if question.choice === 'single'}
		<RadioGroup.Root
			value={answer.selected[0] ?? ''}
			onValueChange={(value) => onchange({ selected: value ? [value] : [], text: answer.text }, false)}
			class="grid gap-2"
			{disabled}
			aria-labelledby="q-{question.id}-title"
		>
			{#each question.options as option (option.id)}
				<label class={optionClass} for="o-{option.id}">
					<RadioGroup.Item value={option.id} id="o-{option.id}" class="mt-0.5" />
					{@render optionBody(option)}
				</label>
			{/each}
		</RadioGroup.Root>
	{:else if question.choice === 'multiple'}
		<div class="grid gap-2" role="group" aria-labelledby="q-{question.id}-title">
			{#each question.options as option (option.id)}
				<label class={optionClass} for="o-{option.id}">
					<Checkbox
						id="o-{option.id}"
						class="mt-0.5"
						checked={answer.selected.includes(option.id)}
						onCheckedChange={(checked) => toggle(option.id, checked === true)}
						{disabled}
					/>
					{@render optionBody(option)}
				</label>
			{/each}
		</div>
	{:else}
		<Textarea
			value={answer.text}
			oninput={(event) => onchange({ selected: [], text: event.currentTarget.value }, true)}
			onblur={ontextblur}
			rows={5}
			placeholder="Ваш ответ"
			{disabled}
			aria-labelledby="q-{question.id}-title"
		/>
		<Badge variant="outline" class="w-fit">автоматически не оценивается</Badge>
	{/if}

	{@render footer?.()}
</section>

{#snippet optionBody(option: Schema<'RunnerOption'>)}
	<span class="grid min-w-0 flex-1 gap-2">
		{#if option.body}
			<Markdown source={option.body} />
		{/if}
		{#if option.image}
			<img
				src={option.image.url}
				alt={option.image.alt || 'Вариант-картинка'}
				class="max-h-64 w-auto max-w-full rounded-md border object-contain"
				loading="lazy"
			/>
		{/if}
	</span>
{/snippet}
