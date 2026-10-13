<script lang="ts">
	import { page } from '$app/state';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import ClockAlertIcon from '@lucide/svelte/icons/clock-alert';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import FormField from '$lib/components/app/FormField.svelte';
	import SubmitButton from '$lib/components/app/SubmitButton.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { ApiError, api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { PASSWORD_MIN, validateNewPassword, validateRepeat } from '$lib/validation';

	const token = $derived(page.url.searchParams.get('token')?.trim() || null);

	let password = $state('');
	let repeat = $state('');
	let submitted = $state(false);
	let pending = $state(false);
	let done = $state(false);
	let expired = $state(false);
	let formError = $state<string | null>(null);

	const errors = $derived({
		password: submitted ? validateNewPassword(password) : null,
		repeat: submitted ? validateRepeat(password, repeat) : null
	});

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (!token) return;
		submitted = true;
		formError = null;
		if (errors.password || errors.repeat) return;
		pending = true;
		try {
			await unwrap(api.POST('/api/auth/password/reset', { body: { token, password } }));
			done = true;
			// Сервер завершил все сессии — обновим состояние шапки.
			await session.load();
		} catch (error) {
			if (error instanceof ApiError && error.status === 410) expired = true;
			else formError = errorMessage(error);
		} finally {
			pending = false;
		}
	}
</script>

<svelte:head>
	<title>Новый пароль — Зачтено</title>
</svelte:head>

{#if !token}
	<AuthCard title="Ссылка неполная">
		<Alert.Root variant="destructive">
			<CircleAlertIcon />
			<Alert.Description>
				В ссылке нет кода восстановления. Откройте ссылку из письма целиком или запросите новую.
			</Alert.Description>
		</Alert.Root>
		<Button href="/forgot-password" class="mt-4 w-full">Запросить новую ссылку</Button>
	</AuthCard>
{:else if expired}
	<AuthCard title="Ссылка устарела">
		<div class="flex flex-col items-center gap-3 text-center">
			<ClockAlertIcon class="text-muted-foreground size-10" />
			<p>Ссылка для восстановления действует 1 час. Запросите новую — это займёт минуту.</p>
			<Button href="/forgot-password" class="mt-2 w-full">Отправить новую ссылку</Button>
		</div>
	</AuthCard>
{:else if done}
	<AuthCard title="Пароль изменён">
		<div class="flex flex-col items-center gap-3 text-center">
			<CircleCheckIcon class="size-10 text-emerald-600 dark:text-emerald-400" />
			<p>Новый пароль сохранён. Все сессии завершены — войдите заново на нужных устройствах.</p>
			<Button href="/login" class="mt-2 w-full">Войти</Button>
		</div>
	</AuthCard>
{:else}
	<AuthCard title="Новый пароль" description="Придумайте новый пароль для входа.">
		<form class="grid gap-4" novalidate onsubmit={submit}>
			{#if formError}
				<Alert.Root variant="destructive" aria-live="polite">
					<CircleAlertIcon />
					<Alert.Description>{formError}</Alert.Description>
				</Alert.Root>
			{/if}
			<FormField
				id="password"
				label="Новый пароль"
				type="password"
				autocomplete="new-password"
				hint="Не короче {PASSWORD_MIN} символов, состав символов любой."
				bind:value={password}
				error={errors.password}
			/>
			<FormField
				id="repeat"
				label="Повторите пароль"
				type="password"
				autocomplete="new-password"
				bind:value={repeat}
				error={errors.repeat}
			/>
			<SubmitButton {pending} pendingText="Сохраняем…">Сохранить пароль</SubmitButton>
		</form>
	</AuthCard>
{/if}
