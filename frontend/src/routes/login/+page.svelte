<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import FormField from '$lib/components/app/FormField.svelte';
	import SubmitButton from '$lib/components/app/SubmitButton.svelte';
	import YandexButton from '$lib/components/app/YandexButton.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Separator } from '$lib/components/ui/separator';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { safeNext, validateEmail } from '$lib/validation';
	import { oauthErrorMessage } from '$lib/oauth-errors';
	import { isEmailField, sendSignals, TypingRecorder, typingRhythm } from '$lib/signals';

	// Ритм набора — только в поле email; пароль не наблюдается никогда.
	const typing = new TypingRecorder();

	const next = $derived(safeNext(page.url.searchParams.get('next')));
	const oauthError = $derived(oauthErrorMessage(page.url.searchParams.get('oauth_error')));
	const nextQuery = $derived(next === '/' ? '' : `?next=${encodeURIComponent(next)}`);

	let email = $state('');
	let password = $state('');
	let submitted = $state(false);
	let pending = $state(false);
	let formError = $state<string | null>(null);

	const errors = $derived({
		email: submitted ? validateEmail(email) : null,
		password: submitted && password === '' ? 'Введите пароль.' : null
	});

	// Уже вошли — сразу туда, куда шли.
	$effect(() => {
		// С ошибкой OAuth не уводим: например, already_linked приходит вошедшему пользователю.
		if (session.user && !pending && !oauthError) void goto(next, { replaceState: true });
	});

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		formError = null;
		if (errors.email || errors.password) return;
		pending = true;
		try {
			const user = await unwrap(
				api.POST('/api/auth/login', { body: { email: email.trim(), password } })
			);
			session.setUser(user);
			sendSignals('login', typing.snapshot());
			await goto(next, { replaceState: true });
		} catch (error) {
			formError = errorMessage(error);
		} finally {
			pending = false;
		}
	}
</script>

<svelte:head>
	<title>Вход — Зачтено</title>
</svelte:head>

<AuthCard title="Вход" description="Войдите, чтобы проходить тесты и видеть результаты.">
	<form class="grid gap-4" novalidate onsubmit={submit} use:typingRhythm={{ recorder: typing, accept: isEmailField }}>
		{#if oauthError}
			<Alert.Root variant="destructive" aria-live="polite">
				<CircleAlertIcon />
				<Alert.Title>Вход через Яндекс не выполнен</Alert.Title>
				<Alert.Description>
					<p>{oauthError}</p>
					{#if session.user}
						<a href="/profile" class="underline underline-offset-3">Вернуться в профиль</a>
					{/if}
				</Alert.Description>
			</Alert.Root>
		{/if}

		{#if formError}
			<Alert.Root variant="destructive" aria-live="polite">
				<CircleAlertIcon />
				<Alert.Description>{formError}</Alert.Description>
			</Alert.Root>
		{/if}

		<FormField
			id="email"
			label="Email"
			type="email"
			autocomplete="email"
			inputmode="email"
			bind:value={email}
			error={errors.email}
		/>
		<FormField
			id="password"
			label="Пароль"
			type="password"
			autocomplete="current-password"
			bind:value={password}
			error={errors.password}
		>
			{#snippet aside()}
				<a href="/forgot-password" class="text-primary text-sm underline-offset-3 hover:underline">
					Забыли пароль?
				</a>
			{/snippet}
		</FormField>

		<SubmitButton {pending} pendingText="Входим…">Войти</SubmitButton>
	</form>

	{#if session.yandexEnabled}
		<div class="my-5 flex items-center gap-3">
			<Separator class="flex-1" />
			<span class="text-muted-foreground text-xs">или</span>
			<Separator class="flex-1" />
		</div>
		<YandexButton href="/api/auth/oauth/yandex/start{nextQuery}" />
	{/if}

	{#snippet footer()}
		<p>
			Нет аккаунта?
			<a href="/register{nextQuery}" class="text-primary font-medium underline-offset-3 hover:underline">
				Зарегистрироваться
			</a>
		</p>
	{/snippet}
</AuthCard>
