<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import Markdown from '$lib/components/app/Markdown.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import { formatScore } from '$lib/format';
	import { cn } from '$lib/utils';
	import type { Schema } from '$lib/api/client';

	let { stats }: { stats: Schema<'QuestionStat'>[] } = $props();

	function percent(part: number, total: number): number {
		return total > 0 ? Math.round((part / total) * 100) : 0;
	}
</script>

{#if stats.length === 0}
	<EmptyState title="Статистики пока нет" text="Она появится, когда студенты сдадут попытки." />
{:else}
	<p class="text-muted-foreground text-sm">
		Выборка — сколько попыток содержали вопрос (при случайной выборке вопросов она у каждого своя).
		Полосы показывают, в какой доле этих попыток выбирали вариант.
	</p>
	<div class="grid grid-cols-1 gap-4">
		{#each stats as stat (stat.question_id)}
			<Card.Root>
				<Card.Header>
					<Card.Title class="text-muted-foreground text-sm font-medium">Вопрос {stat.number}</Card.Title>
				</Card.Header>
				<Card.Content class="grid gap-4">
					<Markdown source={stat.body} />
					<dl class="grid grid-cols-2 gap-3 text-sm sm:grid-cols-5">
						<div><dt class="text-muted-foreground text-xs">Выборка</dt><dd class="font-medium tabular-nums">{stat.sample}</dd></div>
						<div><dt class="text-muted-foreground text-xs">Ответили</dt><dd class="font-medium tabular-nums">{stat.answered}</dd></div>
						<div>
							<dt class="text-muted-foreground text-xs">Полностью верно</dt>
							<dd class="font-medium tabular-nums">
								{stat.fully_correct}
								<span class="text-muted-foreground text-xs font-normal">({percent(stat.fully_correct, stat.sample)}%)</span>
							</dd>
						</div>
						<div><dt class="text-muted-foreground text-xs">Пропуски</dt><dd class="font-medium tabular-nums">{stat.skipped}</dd></div>
						<div>
							<dt class="text-muted-foreground text-xs">Средний балл</dt>
							<dd class="font-medium tabular-nums">
								{stat.average_score === null || stat.average_score === undefined ? 'не оценивается' : formatScore(stat.average_score)}
							</dd>
						</div>
					</dl>

					{#if stat.options.length > 0}
						<ul class="grid gap-2" aria-label="Частота выбора вариантов">
							{#each stat.options as option (option.option_id)}
								{@const share = percent(option.chosen, stat.sample)}
								<li class="grid gap-1">
									<div class="flex items-start justify-between gap-3 text-sm">
										<span class="flex min-w-0 items-start gap-1.5">
											{#if option.is_correct}
												<CheckIcon class="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" aria-label="Верный вариант" />
											{:else}
												<span class="size-4 shrink-0" aria-hidden="true"></span>
											{/if}
											<span class="min-w-0 break-words">{option.body || '[картинка]'}</span>
										</span>
										<span class="text-muted-foreground shrink-0 text-xs tabular-nums">{option.chosen} · {share}%</span>
									</div>
									<div class="bg-muted h-2 overflow-hidden rounded-full" aria-hidden="true">
										<div
											class={cn('h-full rounded-full', option.is_correct ? 'bg-emerald-500' : 'bg-primary/50')}
											style="width: {share}%"
										></div>
									</div>
								</li>
							{/each}
						</ul>
					{:else}
						<Badge variant="outline" class="w-fit">свободный ответ — без вариантов</Badge>
					{/if}
				</Card.Content>
			</Card.Root>
		{/each}
	</div>
{/if}
