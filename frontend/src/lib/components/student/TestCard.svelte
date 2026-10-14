<script lang="ts">
	import TimerIcon from '@lucide/svelte/icons/timer';
	import RepeatIcon from '@lucide/svelte/icons/repeat';
	import ListOrderedIcon from '@lucide/svelte/icons/list-ordered';
	import CalendarClockIcon from '@lucide/svelte/icons/calendar-clock';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import type { Schema } from '$lib/api/client';
	import { count, formatDateTime, formatScore } from '$lib/format';
	import { reviewStatusText } from '$lib/attempt/review-status';

	type Kind = 'available' | 'scheduled' | 'completed' | 'closed';

	let { test, kind }: { test: Schema<'DashboardTest'>; kind: Kind } = $props();

	const attemptsText = $derived(
		test.attempts_limit === null || test.attempts_limit === undefined
			? `${test.attempts_used} / без ограничения`
			: `${test.attempts_used} / ${test.attempts_limit}`
	);
	const timeText = $derived(
		test.time_limit_minutes ? count(test.time_limit_minutes, ['минута', 'минуты', 'минут']) : 'без лимита времени'
	);
	const exhausted = $derived(
		test.attempts_limit !== null && test.attempts_limit !== undefined && test.attempts_used >= test.attempts_limit
	);
</script>

<Card.Root class="gap-4">
	<Card.Header>
		<p class="text-muted-foreground text-xs font-medium tracking-wide uppercase">{test.course_title}</p>
		<Card.Title class="text-base leading-snug">{test.title}</Card.Title>
		{#if test.description}
			<Card.Description class="line-clamp-2">{test.description}</Card.Description>
		{/if}
	</Card.Header>
	<Card.Content class="grid gap-3">
		<ul class="text-muted-foreground grid gap-1.5 text-sm">
			<li class="flex items-center gap-2"><TimerIcon class="size-4 shrink-0" />{timeText}</li>
			<li class="flex items-center gap-2">
				<RepeatIcon class="size-4 shrink-0" />Попытки: {attemptsText}
			</li>
			<li class="flex items-center gap-2">
				<ListOrderedIcon class="size-4 shrink-0" />{count(test.question_count, ['вопрос', 'вопроса', 'вопросов'])}
			</li>
			{#if kind === 'scheduled' && test.opens_at}
				<li class="text-foreground flex items-center gap-2">
					<CalendarClockIcon class="size-4 shrink-0" />Откроется {formatDateTime(test.opens_at)}
				</li>
			{:else if kind === 'available' && test.closes_at}
				<li class="flex items-center gap-2">
					<CalendarClockIcon class="size-4 shrink-0" />Закроется {formatDateTime(test.closes_at)}
				</li>
			{/if}
		</ul>

		{#if kind === 'completed'}
			<div class="bg-muted/60 grid gap-1 rounded-lg p-3 text-sm">
				{#if test.final_score !== null && test.final_score !== undefined}
					<p>Итоговый балл: <span class="text-base font-semibold">{formatScore(test.final_score)}</span></p>
				{/if}
				<p class={test.review?.available ? 'text-foreground' : 'text-muted-foreground'}>
					{reviewStatusText(test.review, test.is_open)}
				</p>
			</div>
		{/if}
	</Card.Content>
	<Card.Footer class="flex flex-wrap gap-2">
		{#if kind === 'available'}
			{#if test.active_attempt_id}
				<Button href="/attempts/{test.active_attempt_id}">Продолжить</Button>
				<Badge variant="secondary">попытка идёт</Badge>
			{:else if exhausted}
				<Button disabled>Попытки исчерпаны</Button>
			{:else}
				<Button href="/tests/{test.test_id}/start">Начать</Button>
			{/if}
		{:else if kind === 'scheduled' || kind === 'closed'}
			<Button href="/tests/{test.test_id}/start" variant="outline">Подробнее</Button>
			{#if kind === 'closed'}<Badge variant="secondary">откроет преподаватель</Badge>{/if}
		{:else if test.last_submitted_attempt_id}
			<Button href="/attempts/{test.last_submitted_attempt_id}/review" variant={test.review?.available ? 'default' : 'outline'}>
				{test.review?.available ? 'Разбор последней попытки' : 'Последняя попытка'}
			</Button>
		{/if}
	</Card.Footer>
</Card.Root>
