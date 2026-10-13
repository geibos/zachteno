<script lang="ts">
	import MailWarningIcon from '@lucide/svelte/icons/mail-warning';
	import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
	import CalendarIcon from '@lucide/svelte/icons/calendar';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import * as Alert from '$lib/components/ui/alert';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import TestCard from './TestCard.svelte';
	import { api, unwrap, type CurrentUser } from '$lib/api/client';
	import { resource } from '$lib/resource.svelte';

	let { user }: { user: CurrentUser } = $props();

	const dashboard = resource(() => unwrap(api.GET('/api/dashboard')));
</script>

<div class="mx-auto grid w-full max-w-5xl grid-cols-1 gap-8 px-4 py-8 sm:px-6">
	<div>
		<h1 class="text-2xl font-semibold tracking-tight">Мои тесты</h1>
		<p class="text-muted-foreground mt-1">Здравствуйте, {user.display_name}!</p>
	</div>

	{#if !user.email_verified}
		<Alert.Root>
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

	<LoadState resource={dashboard}>
		{#snippet children(data)}
			{#if data.available.length + data.scheduled.length + data.completed.length === 0}
				<EmptyState
					icon={ClipboardListIcon}
					title="Тестов пока нет"
					text="Чтобы попасть в поток, перейдите по пригласительной ссылке от преподавателя."
				/>
			{:else}
				<section class="grid gap-3" aria-labelledby="available-heading">
					<h2 id="available-heading" class="flex items-center gap-2 text-lg font-semibold">
						<ClipboardListIcon class="text-primary size-5" />Доступные сейчас
					</h2>
					{#if data.available.length === 0}
						<p class="text-muted-foreground text-sm">Сейчас открытых тестов нет.</p>
					{:else}
						<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
							{#each data.available as test (test.test_id)}
								<TestCard {test} kind="available" />
							{/each}
						</div>
					{/if}
				</section>

				{#if data.scheduled.length > 0}
					<section class="grid gap-3" aria-labelledby="scheduled-heading">
						<h2 id="scheduled-heading" class="flex items-center gap-2 text-lg font-semibold">
							<CalendarIcon class="text-primary size-5" />Запланированные
						</h2>
						<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
							{#each data.scheduled as test (test.test_id)}
								<TestCard {test} kind="scheduled" />
							{/each}
						</div>
					</section>
				{/if}

				{#if data.completed.length > 0}
					<section class="grid gap-3" aria-labelledby="completed-heading">
						<h2 id="completed-heading" class="flex items-center gap-2 text-lg font-semibold">
							<CircleCheckIcon class="text-primary size-5" />Пройденные
						</h2>
						<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
							{#each data.completed as test (test.test_id)}
								<TestCard {test} kind="completed" />
							{/each}
						</div>
					</section>
				{/if}
			{/if}
		{/snippet}
	</LoadState>
</div>
