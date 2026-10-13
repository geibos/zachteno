<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import InfoIcon from '@lucide/svelte/icons/info';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import RegisterForm from '$lib/components/app/RegisterForm.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { session } from '$lib/api/session.svelte';
	import { safeNext } from '$lib/validation';

	const next = $derived(safeNext(page.url.searchParams.get('next')));
	const trusted = $derived(page.url.searchParams.get('trusted')?.trim() || null);

	const loginHref = $derived(next === '/' ? '/login' : `/login?next=${encodeURIComponent(next)}`);
	const yandexHref = $derived.by(() => {
		const params = new URLSearchParams();
		if (next !== '/') params.set('next', next);
		if (trusted) params.set('trusted', trusted);
		const query = params.toString();
		return '/api/auth/oauth/yandex/start' + (query ? `?${query}` : '');
	});

	// Пока идёт регистрация, не уводим со страницы раньше времени.
	let registering = $state(false);

	// Уже вошли без доверенной ссылки — регистрация не нужна.
	$effect(() => {
		if (session.user && !trusted && !registering) void goto(next, { replaceState: true });
	});

	async function onregistered(): Promise<void> {
		await goto(next, { replaceState: true });
	}
</script>

<svelte:head>
	<title>Регистрация — Зачтено</title>
</svelte:head>

{#if session.user && trusted && !registering}
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
		<RegisterForm {trusted} {loginHref} {yandexHref} {onregistered} onpending={(value) => (registering = value)}>
			{#snippet banner()}
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
			{/snippet}
		</RegisterForm>

		{#snippet footer()}
			<p>
				Уже есть аккаунт?
				<a href={loginHref} class="text-primary font-medium underline-offset-3 hover:underline">Войти</a>
			</p>
		{/snippet}
	</AuthCard>
{/if}
