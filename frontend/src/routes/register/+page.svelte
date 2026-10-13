<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import InfoIcon from '@lucide/svelte/icons/info';
	import { toast } from 'svelte-sonner';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import FormField from '$lib/components/app/FormField.svelte';
	import SubmitButton from '$lib/components/app/SubmitButton.svelte';
	import YandexButton from '$lib/components/app/YandexButton.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { Separator } from '$lib/components/ui/separator';
	import { ApiError, api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import {
		PASSWORD_MIN,
		safeNext,
		validateEmail,
		validateName,
		validateNewPassword,
		validateRepeat
	} from '$lib/validation';

	const next = $derived(safeNext(page.url.searchParams.get('next')));
	const trusted = $derived(page.url.searchParams.get('trusted')?.trim() || null);

	const loginQuery = $derived(next === '/' ? '' : `?next=${encodeURIComponent(next)}`);
	const yandexHref = $derived.by(() => {
		const params = new URLSearchParams();
		if (next !== '/') params.set('next', next);
		if (trusted) params.set('trusted', trusted);
		const query = params.toString();
		return '/api/auth/oauth/yandex/start' + (query ? `?${query}` : '');
	});

	let email = $state('');
	let name = $state('');
	let password = $state('');
	let repeat = $state('');
	let submitted = $state(false);
	let pending = $state(false);
	let formError = $state<string | null>(null);
	let emailTaken = $state(false);

	const errors = $derived({
		email: submitted ? validateEmail(email) : null,
		name: submitted ? validateName(name) : null,
		password: submitted ? validateNewPassword(password) : null,
		repeat: submitted ? validateRepeat(password, repeat) : null
	});

	// Уже вошли без доверенной ссылки — регистрация не нужна.
	$effect(() => {
		if (session.user && !trusted && !pending) void goto(next, { replaceState: true });
	});

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		formError = null;
		emailTaken = false;
		if (Object.values(errors).some(Boolean)) return;
		pending = true;
		try {
			const user = await unwrap(
				api.POST('/api/auth/register', {
					body: {
						email: email.trim(),
						display_name: name.trim(),
						password,
						trusted_token: trusted
					}
				})
			);
			session.setUser(user);
			toast.success('Учётная запись создана', {
				description: `Мы отправили письмо для подтверждения на ${user.email}.`
			});
			await goto(next, { replaceState: true });
		} catch (error) {
			emailTaken = error instanceof ApiError && error.status === 409;
			formError = errorMessage(error);
		} finally {
			pending = false;
		}
	}
</script>

<svelte:head>
	<title>Регистрация — Зачтено</title>
</svelte:head>

{#if session.user && trusted && !pending}
	<AuthCard title="Вы уже вошли">
		<Alert.Root>
			<InfoIcon />
			<Alert.Title>Ссылка предназначена для новой регистрации</Alert.Title>
			<Alert.Description>
				Вы вошли как {session.user.email}. Отметка «доверенная регистрация» к существующей
				учётной записи автоматически не добавляется.
			</Alert.Description>
		</Alert.Root>
		<Button href="/" variant="outline" class="mt-4 w-full">На главную</Button>
	</AuthCard>
{:else}
	<AuthCard title="Регистрация" description="Создайте учётную запись, чтобы проходить тесты.">
		<form class="grid gap-4" novalidate onsubmit={submit}>
			{#if trusted}
				<Alert.Root class="border-primary/30 bg-primary/5">
					<ShieldCheckIcon class="text-primary" />
					<Alert.Title>Доверенная регистрация</Alert.Title>
					<Alert.Description>
						Вы регистрируетесь по ссылке администратора — учётная запись получит отметку
						«доверенная регистрация».
					</Alert.Description>
				</Alert.Root>
			{/if}

			{#if formError}
				<Alert.Root variant="destructive" aria-live="polite">
					<CircleAlertIcon />
					<Alert.Description>
						<p>{formError}</p>
						{#if emailTaken}
							<p class="mt-1">
								<a href="/login{loginQuery}" class="underline underline-offset-3">Войти</a>
								или
								<a href="/forgot-password" class="underline underline-offset-3">восстановить пароль</a>
							</p>
						{/if}
					</Alert.Description>
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
				id="name"
				label="Имя"
				autocomplete="name"
				hint="Так вас увидят преподаватели, например «Анна Петрова»."
				bind:value={name}
				error={errors.name}
			/>
			<FormField
				id="password"
				label="Пароль"
				type="password"
				autocomplete="new-password"
				hint="Не короче {PASSWORD_MIN} символов. Цифры и спецсимволы не обязательны — важна длина, подойдёт фраза из нескольких слов."
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

			<SubmitButton {pending} pendingText="Создаём учётную запись…">Зарегистрироваться</SubmitButton>
		</form>

		{#if session.yandexEnabled}
			<div class="my-5 flex items-center gap-3">
				<Separator class="flex-1" />
				<span class="text-muted-foreground text-xs">или</span>
				<Separator class="flex-1" />
			</div>
			<YandexButton href={yandexHref} label="Продолжить с Яндекс ID" />
		{/if}

		{#snippet footer()}
			<p>
				Уже есть аккаунт?
				<a href="/login{loginQuery}" class="text-primary font-medium underline-offset-3 hover:underline">Войти</a>
			</p>
		{/snippet}
	</AuthCard>
{/if}
