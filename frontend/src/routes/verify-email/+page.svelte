<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import ClockAlertIcon from '@lucide/svelte/icons/clock-alert';
	import MailCheckIcon from '@lucide/svelte/icons/mail-check';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { ApiError, api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';

	type State = 'verifying' | 'success' | 'expired' | 'invalid' | 'error';

	// Токен читаем один раз: страница не должна повторно отправлять его при навигации.
	const token = page.url.searchParams.get('token')?.trim() || null;

	let status = $state<State>(token ? 'verifying' : 'invalid');
	let message = $state<string | null>(token ? null : 'В ссылке нет кода подтверждения.');
	let resendPending = $state(false);
	let resent = $state(false);
	let resendError = $state<string | null>(null);

	async function verify(): Promise<void> {
		if (!token) return;
		status = 'verifying';
		try {
			await unwrap(api.POST('/api/auth/verify-email', { body: { token } }));
			status = 'success';
			if (session.user) await session.load();
		} catch (error) {
			message = errorMessage(error);
			status =
				error instanceof ApiError && error.status === 410 ? 'expired'
				: error instanceof ApiError && error.status === 400 ? 'invalid'
				: 'error';
		}
	}

	async function resend(): Promise<void> {
		resendPending = true;
		resendError = null;
		try {
			await unwrap(api.POST('/api/auth/verify-email/resend'));
			resent = true;
		} catch (error) {
			resendError = errorMessage(error);
		} finally {
			resendPending = false;
		}
	}

	onMount(() => {
		void verify();
	});
</script>

<svelte:head>
	<title>Подтверждение email — Зачтено</title>
</svelte:head>

{#snippet resendBlock()}
	{#if session.user?.email_verified}
		<p class="text-sm">Ваш email уже подтверждён — ничего делать не нужно.</p>
		<Button href="/" variant="outline" class="w-full">Перейти в панель</Button>
	{:else if resent}
		<Alert.Root>
			<MailCheckIcon />
			<Alert.Description>Новое письмо отправлено на {session.user?.email}.</Alert.Description>
		</Alert.Root>
	{:else if session.user}
		{#if resendError}
			<p class="text-destructive text-sm" aria-live="polite">{resendError}</p>
		{/if}
		<Button class="w-full" onclick={resend} disabled={resendPending}>
			{#if resendPending}<LoaderIcon class="animate-spin" />Отправляем…{:else}Отправить новую{/if}
		</Button>
	{:else}
		<p class="text-muted-foreground text-sm">Войдите, чтобы получить новую ссылку.</p>
		<Button href="/login?next={encodeURIComponent('/profile')}" class="w-full">Войти и отправить новую</Button>
	{/if}
{/snippet}

{#if status === 'verifying'}
	<AuthCard title="Подтверждаем email">
		<div class="text-muted-foreground flex items-center justify-center gap-2 py-6" role="status">
			<LoaderIcon class="size-5 animate-spin" />
			Проверяем ссылку…
		</div>
	</AuthCard>
{:else if status === 'success'}
	<AuthCard title="Email подтверждён">
		<div class="flex flex-col items-center gap-3 text-center">
			<CircleCheckIcon class="size-10 text-emerald-600 dark:text-emerald-400" />
			<p>Спасибо! Адрес подтверждён.</p>
			<Button href={session.user ? '/' : '/login'} class="mt-2 w-full">
				{session.user ? 'Перейти в панель' : 'Войти'}
			</Button>
		</div>
	</AuthCard>
{:else if status === 'expired'}
	<AuthCard title="Ссылка устарела">
		<div class="flex flex-col items-center gap-3 text-center">
			<ClockAlertIcon class="text-muted-foreground size-10" />
			<p>Ссылка подтверждения действует 24 часа. Отправим новую?</p>
			<div class="grid w-full gap-2">{@render resendBlock()}</div>
		</div>
	</AuthCard>
{:else}
	<AuthCard title={status === 'invalid' ? 'Ссылка недействительна' : 'Не удалось подтвердить email'}>
		<div class="grid gap-3">
			<Alert.Root variant="destructive">
				<CircleAlertIcon />
				<Alert.Description>{message}</Alert.Description>
			</Alert.Root>
			{#if status === 'error'}
				<Button variant="outline" class="w-full" onclick={verify}>Повторить</Button>
			{:else}
				<p class="text-muted-foreground text-sm">
					Возможно, ссылка уже использована или скопирована не полностью.
				</p>
				{@render resendBlock()}
			{/if}
		</div>
	</AuthCard>
{/if}
