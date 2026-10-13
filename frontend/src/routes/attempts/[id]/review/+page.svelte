<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/state';
	import CheckIcon from '@lucide/svelte/icons/check';
	import XIcon from '@lucide/svelte/icons/x';
	import ClockIcon from '@lucide/svelte/icons/clock';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import Markdown from '$lib/components/app/Markdown.svelte';
	import * as Card from '$lib/components/ui/card';
	import * as Alert from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { api, unwrap, type Schema } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import { formatDateTime, formatScore } from '$lib/format';
	import { reviewStatusText, submitReasonText } from '$lib/attempt/review-status';
	import { cn } from '$lib/utils';

	const attemptId = $derived(page.params.id ?? '');

	const review = new Resource(() =>
		unwrap(api.GET('/api/attempts/{attempt_id}/review', { params: { path: { attempt_id: attemptId } } }))
	);

	$effect(() => {
		void attemptId;
		void review.reload();
	});

	// 409 «ещё не сдана»: сразу после автосдачи сервер может не успеть — повторяем.
	let retries = 0;
	let retryTimer: ReturnType<typeof setTimeout> | null = null;
	const notSubmitted = $derived(review.status === 409);
	$effect(() => {
		if (notSubmitted && retries < 12 && retryTimer === null) {
			retryTimer = setTimeout(() => {
				retryTimer = null;
				retries += 1;
				void review.reload();
			}, 5000);
		}
	});
	onDestroy(() => {
		if (retryTimer !== null) clearTimeout(retryTimer);
	});

	function optionState(option: Schema<'ReviewOption'>): 'right' | 'wrong' | 'missed' | 'none' {
		if (option.selected && option.is_correct) return 'right';
		if (option.selected) return 'wrong';
		if (option.is_correct) return 'missed';
		return 'none';
	}
</script>

<svelte:head>
	<title>Результаты — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-3xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
			{#if notSubmitted && review.data === null}
				<PageHeader title="Попытка сдаётся" back={{ href: '/', label: 'Мои тесты' }} />
				<Alert.Root>
					<LoaderIcon class="animate-spin" />
					<Alert.Title>Подождите немного</Alert.Title>
					<Alert.Description>
						<p>Сервер ещё завершает попытку. Страница обновится сама.</p>
						<Button href="/attempts/{attemptId}" variant="outline" size="sm" class="mt-2">Вернуться к попытке</Button>
					</Alert.Description>
				</Alert.Root>
			{:else}
				<LoadState resource={review}>
					{#snippet children(data)}
						{@const reason = submitReasonText(data.submit_reason)}
						<PageHeader title={data.title} back={{ href: '/', label: 'Мои тесты' }}>
							{#snippet badges()}
								{#if data.submitted_at}
									<span class="text-muted-foreground text-sm">Сдана {formatDateTime(data.submitted_at)}</span>
								{/if}
								{#if reason}<Badge variant="secondary">{reason}</Badge>{/if}
							{/snippet}
						</PageHeader>

						{#if !data.availability.available}
							<Card.Root>
								<Card.Content class="flex flex-col items-center gap-3 py-8 text-center">
									<ClockIcon class="text-muted-foreground size-10" />
									<p class="font-medium">
										Попытка сдана{data.submitted_at ? ` ${formatDateTime(data.submitted_at)}` : ''}.
									</p>
									<p class="text-muted-foreground max-w-md text-sm">{reviewStatusText(data.availability)}.</p>
									<Button href="/" variant="outline" class="mt-2">К моим тестам</Button>
								</Card.Content>
							</Card.Root>
						{:else}
							<Card.Root>
								<Card.Content class="flex flex-wrap items-end justify-between gap-4">
									<div>
										<p class="text-muted-foreground text-sm">Балл за попытку</p>
										<p class="text-3xl font-semibold tabular-nums">
											{formatScore(data.score)}
											<span class="text-muted-foreground text-lg font-normal">из {formatScore(data.max_score)}</span>
										</p>
									</div>
									<div class="text-muted-foreground grid gap-1 text-xs">
										<span class="inline-flex items-center gap-1.5">
											<CheckIcon class="size-3.5 text-emerald-600 dark:text-emerald-400" />ваш ответ верный
										</span>
										<span class="inline-flex items-center gap-1.5">
											<XIcon class="text-destructive size-3.5" />ваш ответ неверный
										</span>
										<span class="inline-flex items-center gap-1.5">
											<EyeIcon class="size-3.5 text-emerald-600 dark:text-emerald-400" />правильный, но не выбран
										</span>
									</div>
								</Card.Content>
							</Card.Root>

							{#each data.questions as q (q.index)}
								<Card.Root>
									<Card.Header>
										<div class="flex flex-wrap items-center justify-between gap-2">
											<Card.Title class="text-muted-foreground text-sm font-medium">Вопрос {q.index + 1}</Card.Title>
											{#if q.choice === 'free_text' || q.score === null || q.score === undefined}
												<Badge variant="outline">не оценивается</Badge>
											{:else}
												<Badge
													variant={Number(q.score) >= Number(q.weight) ? 'default' : Number(q.score) > 0 ? 'secondary' : 'destructive'}
												>
													{formatScore(q.score)} из {formatScore(q.weight)}
												</Badge>
											{/if}
										</div>
									</Card.Header>
									<Card.Content class="grid gap-4">
										<Markdown source={q.body} />
										{#if q.image}
											<img src={q.image.url} alt={q.image.alt} class="max-h-80 w-auto max-w-full rounded-lg border object-contain" loading="lazy" />
										{/if}

										{#if q.choice === 'free_text'}
											<div class="grid gap-1">
												<p class="text-muted-foreground text-xs">Ваш ответ</p>
												<p class="bg-muted/60 rounded-md p-3 text-sm whitespace-pre-wrap">
													{q.text_answer?.trim() ? q.text_answer : 'Нет ответа'}
												</p>
											</div>
										{:else}
											<ul class="grid gap-2">
												{#each q.options as option (option.id)}
													{@const state = optionState(option)}
													<li
														class={cn(
															'flex items-start gap-3 rounded-lg border p-3',
															state === 'right' && 'border-emerald-500/50 bg-emerald-500/5',
															state === 'wrong' && 'border-destructive/50 bg-destructive/5',
															state === 'missed' && 'border-dashed border-emerald-500/60'
														)}
													>
														<span class="mt-0.5 shrink-0" aria-hidden="true">
															{#if state === 'right'}
																<CheckIcon class="size-4 text-emerald-600 dark:text-emerald-400" />
															{:else if state === 'wrong'}
																<XIcon class="text-destructive size-4" />
															{:else if state === 'missed'}
																<EyeIcon class="size-4 text-emerald-600 dark:text-emerald-400" />
															{:else}
																<span class="block size-4"></span>
															{/if}
														</span>
														<span class="grid min-w-0 flex-1 gap-2">
															{#if option.body}<Markdown source={option.body} />{/if}
															{#if option.image}
																<img src={option.image.url} alt={option.image.alt || 'Вариант-картинка'} class="max-h-48 w-auto max-w-full rounded-md border object-contain" loading="lazy" />
															{/if}
															<span class="sr-only">
																{option.selected ? 'Выбран вами.' : ''}
																{option.is_correct ? 'Правильный вариант.' : ''}
															</span>
														</span>
														{#if option.selected}
															<Badge variant="outline" class="shrink-0">ваш выбор</Badge>
														{/if}
													</li>
												{/each}
											</ul>
										{/if}
									</Card.Content>
								</Card.Root>
							{/each}

							<Button href="/" variant="outline" class="w-fit">К моим тестам</Button>
						{/if}
					{/snippet}
				</LoadState>
			{/if}
		</div>
	{/snippet}
</RequireAuth>
