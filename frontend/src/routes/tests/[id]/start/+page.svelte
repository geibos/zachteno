<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import TimerIcon from '@lucide/svelte/icons/timer';
	import RepeatIcon from '@lucide/svelte/icons/repeat';
	import ListOrderedIcon from '@lucide/svelte/icons/list-ordered';
	import RouteIcon from '@lucide/svelte/icons/route';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import InfoIcon from '@lucide/svelte/icons/info';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import * as Card from '$lib/components/ui/card';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import { count, formatDateTime } from '$lib/format';
	import { sendSignals } from '$lib/signals';

	type DashboardTest = Schema<'DashboardTest'>;

	const testId = $derived(page.params.id ?? '');

	// Отдельного эндпоинта «тест для студента» нет: берём карточку из панели.
	const info = new Resource<{ test: DashboardTest; group: 'available' | 'scheduled' | 'completed' | 'closed' } | null>(
		async () => {
			const dashboard = await unwrap(api.GET('/api/dashboard'));
			for (const group of ['available', 'scheduled', 'completed', 'closed'] as const) {
				const test = dashboard[group].find((item) => item.test_id === testId);
				if (test) return { test, group };
			}
			return null;
		}
	);

	$effect(() => {
		void testId;
		void info.reload();
	});

	let pending = $state(false);
	let startError = $state<string | null>(null);

	async function start(): Promise<void> {
		pending = true;
		startError = null;
		try {
			const started = await unwrap(
				api.POST('/api/tests/{test_id}/attempts', { params: { path: { test_id: testId } } })
			);
			if (!started.resumed) sendSignals('attempt_start');
			await goto(`/attempts/${started.attempt_id}`);
		} catch (error) {
			startError = errorMessage(error);
			void info.reload();
		} finally {
			pending = false;
		}
	}

	function exhausted(test: DashboardTest): boolean {
		return test.attempts_limit !== null && test.attempts_limit !== undefined && test.attempts_used >= test.attempts_limit;
	}
</script>

<svelte:head>
	<title>{info.data?.test.title ?? 'Тест'} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-2xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
			<LoadState resource={info}>
				{#snippet children(found)}
					{#if !found}
						<PageHeader title="Тест недоступен" back={{ href: '/', label: 'Мои тесты' }} />
						<Alert.Root>
							<InfoIcon />
							<Alert.Description>
								Этого теста нет среди ваших: возможно, он ещё не назначен вашему потоку или уже закрыт.
							</Alert.Description>
						</Alert.Root>
					{:else}
						{@const test = found.test}
						<PageHeader title={test.title} description={test.course_title} back={{ href: '/', label: 'Мои тесты' }} />
						<Card.Root>
							<Card.Content class="grid gap-5">
								{#if test.description}
									<p class="whitespace-pre-line">{test.description}</p>
								{/if}
								<ul class="grid gap-3 text-sm sm:grid-cols-2">
									<li class="flex items-start gap-2">
										<TimerIcon class="text-muted-foreground mt-0.5 size-4 shrink-0" />
										<span>
											<span class="text-muted-foreground block text-xs">Лимит времени</span>
											{test.time_limit_minutes
												? count(test.time_limit_minutes, ['минута', 'минуты', 'минут'])
												: 'без ограничения'}
										</span>
									</li>
									<li class="flex items-start gap-2">
										<RepeatIcon class="text-muted-foreground mt-0.5 size-4 shrink-0" />
										<span>
											<span class="text-muted-foreground block text-xs">Попытки</span>
											использовано {test.attempts_used} из {test.attempts_limit ?? 'неограниченного числа'}
										</span>
									</li>
									<li class="flex items-start gap-2">
										<ListOrderedIcon class="text-muted-foreground mt-0.5 size-4 shrink-0" />
										<span>
											<span class="text-muted-foreground block text-xs">Вопросов в попытке</span>
											{test.question_count}
										</span>
									</li>
									<li class="flex items-start gap-2">
										<RouteIcon class="text-muted-foreground mt-0.5 size-4 shrink-0" />
										<span>
											<span class="text-muted-foreground block text-xs">Навигация</span>
											{test.free_navigation
												? 'свободная: можно пропускать вопросы и возвращаться'
												: 'по порядку: вернуться к предыдущему вопросу нельзя'}
										</span>
									</li>
								</ul>

								{#if startError}
									<Alert.Root variant="destructive" aria-live="polite">
										<CircleAlertIcon />
										<Alert.Description>{startError}</Alert.Description>
									</Alert.Root>
								{/if}

								{#if test.active_attempt_id}
									<Button href="/attempts/{test.active_attempt_id}" size="lg">Продолжить попытку</Button>
								{:else if found.group === 'scheduled' || !test.is_open}
									<Alert.Root>
										<InfoIcon />
										<Alert.Description>
											{test.opens_at
												? `Тест откроется ${formatDateTime(test.opens_at)}.`
												: 'Тест пока закрыт для вашего потока: его откроет преподаватель.'}
										</Alert.Description>
									</Alert.Root>
								{:else if exhausted(test)}
									<Alert.Root>
										<InfoIcon />
										<Alert.Description>Все попытки использованы — начать новую нельзя.</Alert.Description>
									</Alert.Root>
									<Button size="lg" disabled>Начать</Button>
								{:else}
									<p class="text-muted-foreground text-sm">
										Нажатие «Начать» расходует одну попытку.
										{#if test.time_limit_minutes}
											Таймер пойдёт сразу и не остановится, даже если закрыть вкладку.
										{/if}
										{#if test.closes_at}
											Тест закрывается {formatDateTime(test.closes_at)}.
										{/if}
									</p>
									<Button size="lg" onclick={start} disabled={pending}>
										{#if pending}<LoaderIcon class="animate-spin" />Начинаем…{:else}Начать{/if}
									</Button>
								{/if}

								{#if test.last_submitted_attempt_id}
									<Button href="/attempts/{test.last_submitted_attempt_id}/review" variant="outline">
										Результаты последней попытки
									</Button>
								{/if}
							</Card.Content>
						</Card.Root>
					{/if}
				{/snippet}
			</LoadState>
		</div>
	{/snippet}
</RequireAuth>
