<script lang="ts">
	import TimerIcon from '@lucide/svelte/icons/timer';
	import SigmaIcon from '@lucide/svelte/icons/sigma';
	import CloudCheckIcon from '@lucide/svelte/icons/cloud-check';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import DashboardView from '$lib/components/student/DashboardView.svelte';
	import { session } from '$lib/api/session.svelte';

	const features = [
		{
			icon: TimerIcon,
			title: 'Честные сроки',
			text: 'Время попытки считает сервер: таймер не собьётся, даже если на устройстве неверные часы.'
		},
		{
			icon: CloudCheckIcon,
			title: 'Ответы не теряются',
			text: 'Каждый ответ сохраняется сразу. Пропала связь — отправим, как только она вернётся.'
		},
		{
			icon: SigmaIcon,
			title: 'Формулы и картинки',
			text: 'Вопросы с формулами, иллюстрациями и вариантами-картинками выглядят как в учебнике.'
		}
	];
</script>

<svelte:head>
	<title>{session.user ? 'Мои тесты — Зачтено' : 'Зачтено — тестирование студентов'}</title>
</svelte:head>

{#if session.user}
	<DashboardView user={session.user} />
{:else}
	<section class="mx-auto w-full max-w-5xl px-4 pt-14 pb-10 sm:px-6 sm:pt-20">
		<div class="max-w-2xl">
			<h1 class="text-3xl font-semibold tracking-tight text-balance sm:text-5xl">
				Тесты для студентов — <span class="text-primary">без лишних хлопот</span>
			</h1>
			<p class="text-muted-foreground mt-4 text-base text-pretty sm:text-lg">
				«Зачтено» помогает преподавателям проводить контрольные онлайн, а студентам — спокойно
				проходить их с любого устройства и вовремя видеть результаты.
			</p>
			<div class="mt-8 flex flex-col gap-3 sm:flex-row">
				<Button href="/register" size="lg" class="px-5">Зарегистрироваться</Button>
				<Button href="/login" size="lg" variant="outline" class="px-5">Войти</Button>
			</div>
		</div>
	</section>

	<section class="mx-auto grid w-full max-w-5xl gap-4 px-4 pb-16 sm:grid-cols-3 sm:px-6" aria-label="Возможности">
		{#each features as feature (feature.title)}
			<Card.Root>
				<Card.Header>
					<div class="bg-primary/10 text-primary mb-2 flex size-10 items-center justify-center rounded-lg">
						<feature.icon class="size-5" />
					</div>
					<Card.Title>{feature.title}</Card.Title>
					<Card.Description>{feature.text}</Card.Description>
				</Card.Header>
			</Card.Root>
		{/each}
	</section>
{/if}
