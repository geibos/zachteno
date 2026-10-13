<script lang="ts">
	import TimerIcon from '@lucide/svelte/icons/timer';
	import SigmaIcon from '@lucide/svelte/icons/sigma';
	import CloudCheckIcon from '@lucide/svelte/icons/cloud-check';
	import MailWarningIcon from '@lucide/svelte/icons/mail-warning';
	import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as Alert from '$lib/components/ui/alert';
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
	<title>{session.user ? 'Панель — Зачтено' : 'Зачтено — тестирование студентов'}</title>
</svelte:head>

{#if session.user}
	{@const user = session.user}
	<div class="mx-auto w-full max-w-5xl px-4 py-8 sm:px-6">
		<h1 class="text-2xl font-semibold tracking-tight">Панель</h1>
		<p class="text-muted-foreground mt-1">Здравствуйте, {user.display_name}!</p>

		{#if !user.email_verified}
			<Alert.Root class="mt-6">
				<MailWarningIcon />
				<Alert.Title>Подтвердите email</Alert.Title>
				<Alert.Description>
					<p>
						Мы отправили письмо на {user.email}. Проходить тесты можно и без подтверждения.
						<a href="/profile" class="text-primary underline underline-offset-3">Отправить ещё раз</a>
					</p>
				</Alert.Description>
			</Alert.Root>
		{/if}

		<Card.Root class="mt-6">
			<Card.Content class="flex flex-col items-center gap-3 py-10 text-center">
				<ClipboardListIcon class="text-muted-foreground size-10" />
				<p class="font-medium">Здесь появятся ваши тесты</p>
				<p class="text-muted-foreground max-w-sm text-sm">
					Доступные, запланированные и пройденные тесты будут собраны на этой странице.
					Чтобы попасть в поток, перейдите по пригласительной ссылке от преподавателя.
				</p>
			</Card.Content>
		</Card.Root>
	</div>
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
