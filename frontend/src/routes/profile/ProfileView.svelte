<script lang="ts">
	import { onMount } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import MailWarningIcon from '@lucide/svelte/icons/mail-warning';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import MonitorSmartphoneIcon from '@lucide/svelte/icons/monitor-smartphone';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import { toast } from 'svelte-sonner';
	import FormField from '$lib/components/app/FormField.svelte';
	import IdentitiesCard from './IdentitiesCard.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { api, errorMessage, unwrap, type CurrentUser, type SessionInfo } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { describeUserAgent, formatDateTime } from '$lib/format';
	import { PASSWORD_MIN, validateNewPassword, validateRepeat } from '$lib/validation';

	let { user }: { user: CurrentUser } = $props();

	// --- подтверждение email ---
	let resendPending = $state(false);
	let resent = $state(false);

	async function resendVerification(): Promise<void> {
		resendPending = true;
		try {
			await unwrap(api.POST('/api/auth/verify-email/resend'));
			resent = true;
			toast.success('Письмо отправлено', { description: `Проверьте почту ${user.email}.` });
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			resendPending = false;
		}
	}

	// --- пароль ---
	let currentPassword = $state('');
	let newPassword = $state('');
	let repeatPassword = $state('');
	let passwordSubmitted = $state(false);
	let passwordPending = $state(false);
	let passwordError = $state<string | null>(null);

	const passwordErrors = $derived({
		current: passwordSubmitted && user.has_password && currentPassword === '' ? 'Введите текущий пароль.' : null,
		next: passwordSubmitted ? validateNewPassword(newPassword) : null,
		repeat: passwordSubmitted ? validateRepeat(newPassword, repeatPassword) : null
	});

	async function changePassword(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		passwordSubmitted = true;
		passwordError = null;
		if (passwordErrors.current || passwordErrors.next || passwordErrors.repeat) return;
		passwordPending = true;
		const hadPassword = user.has_password;
		try {
			await unwrap(
				api.POST('/api/profile/password', {
					body: {
						current_password: hadPassword ? currentPassword : null,
						new_password: newPassword
					}
				})
			);
			currentPassword = newPassword = repeatPassword = '';
			passwordSubmitted = false;
			toast.success(hadPassword ? 'Пароль изменён' : 'Пароль задан', {
				description: 'Остальные сессии завершены.'
			});
			await Promise.all([session.load(), loadSessions()]);
		} catch (error) {
			passwordError = errorMessage(error);
		} finally {
			passwordPending = false;
		}
	}

	// --- сессии ---
	let sessions = $state<SessionInfo[] | null>(null);
	let sessionsError = $state<string | null>(null);
	let revoking = $state<string | null>(null);

	async function loadSessions(): Promise<void> {
		sessionsError = null;
		try {
			const list = await unwrap(api.GET('/api/profile/sessions'));
			sessions = [...list].sort(
				(a, b) => Number(b.current) - Number(a.current) || b.last_seen_at.localeCompare(a.last_seen_at)
			);
		} catch (error) {
			sessionsError = errorMessage(error);
		}
	}

	async function revoke(id: string): Promise<void> {
		revoking = id;
		try {
			await unwrap(api.DELETE('/api/profile/sessions/{session_id}', { params: { path: { session_id: id } } }));
			sessions = sessions?.filter((item) => item.id !== id) ?? null;
			toast.success('Сессия завершена');
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			revoking = null;
		}
	}

	let loggingOut = $state(false);

	async function logout(): Promise<void> {
		loggingOut = true;
		try {
			await session.logout();
			toast.success('Вы вышли из системы');
		} catch (error) {
			toast.error(errorMessage(error));
			loggingOut = false;
		}
	}

	onMount(() => {
		void loadSessions();
	});
</script>

<div class="mx-auto grid w-full max-w-3xl gap-6 px-4 py-8 sm:px-6">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h1 class="text-2xl font-semibold tracking-tight">Профиль</h1>
			<p class="text-muted-foreground mt-1 text-sm">Данные учётной записи, пароль и активные сессии.</p>
		</div>
		<Button variant="outline" onclick={logout} disabled={loggingOut}>
			{#if loggingOut}<LoaderIcon class="animate-spin" />{:else}<LogOutIcon />{/if}
			Выйти
		</Button>
	</div>

	{#if !user.email_verified}
		<Alert.Root class="border-amber-500/40 bg-amber-50 text-amber-950 dark:bg-amber-950/30 dark:text-amber-100">
			<MailWarningIcon />
			<Alert.Title>Подтвердите email</Alert.Title>
			<Alert.Description class="text-current/80">
				<p>
					Мы отправили ссылку на {user.email}. Она действует 24 часа. Проходить тесты можно и без
					подтверждения.
				</p>
				<Button size="sm" variant="outline" class="mt-3" onclick={resendVerification} disabled={resendPending}>
					{#if resendPending}<LoaderIcon class="animate-spin" />Отправляем…{:else if resent}Отправить ещё раз{:else}Отправить письмо повторно{/if}
				</Button>
			</Alert.Description>
		</Alert.Root>
	{/if}

	<Card.Root>
		<Card.Header>
			<Card.Title>Учётная запись</Card.Title>
		</Card.Header>
		<Card.Content>
			<dl class="grid gap-4 sm:grid-cols-[10rem_1fr] sm:gap-x-6 sm:gap-y-3">
				<dt class="text-muted-foreground text-sm">Имя</dt>
				<dd class="-mt-3 font-medium break-words sm:mt-0">{user.display_name}</dd>

				<dt class="text-muted-foreground text-sm">Email</dt>
				<dd class="-mt-3 flex flex-wrap items-center gap-2 sm:mt-0">
					<span class="break-all">{user.email}</span>
					{#if user.email_verified}
						<Badge variant="secondary">подтверждён</Badge>
					{:else}
						<Badge variant="destructive">не подтверждён</Badge>
					{/if}
				</dd>

				<dt class="text-muted-foreground text-sm">Роли</dt>
				<dd class="-mt-3 flex flex-wrap gap-2 sm:mt-0">
					{#if user.roles.superuser}<Badge>суперпользователь</Badge>{/if}
					{#if user.roles.methodist}<Badge>методист</Badge>{/if}
					{#if !user.roles.superuser && !user.roles.methodist}
						<span class="text-muted-foreground text-sm">Глобальных ролей нет</span>
					{/if}
				</dd>

				{#if user.trusted_registration}
					<dt class="text-muted-foreground text-sm">Отметки</dt>
					<dd class="-mt-3 sm:mt-0"><Badge variant="outline">доверенная регистрация</Badge></dd>
				{/if}
			</dl>
		</Card.Content>
	</Card.Root>

	<Card.Root>
		<Card.Header>
			<Card.Title>{user.has_password ? 'Смена пароля' : 'Задать пароль'}</Card.Title>
			<Card.Description>
				{user.has_password
					? 'После смены пароля все остальные сессии будут завершены.'
					: 'Сейчас вы входите только через внешний аккаунт. Задайте пароль, чтобы входить и по email.'}
			</Card.Description>
		</Card.Header>
		<Card.Content>
			<form class="grid max-w-md gap-4" novalidate onsubmit={changePassword}>
				{#if passwordError}
					<Alert.Root variant="destructive" aria-live="polite">
						<CircleAlertIcon />
						<Alert.Description>{passwordError}</Alert.Description>
					</Alert.Root>
				{/if}
				{#if user.has_password}
					<FormField
						id="current-password"
						label="Текущий пароль"
						type="password"
						autocomplete="current-password"
						bind:value={currentPassword}
						error={passwordErrors.current}
					/>
				{/if}
				<FormField
					id="new-password"
					label="Новый пароль"
					type="password"
					autocomplete="new-password"
					hint="Не короче {PASSWORD_MIN} символов, состав символов любой."
					bind:value={newPassword}
					error={passwordErrors.next}
				/>
				<FormField
					id="repeat-password"
					label="Повторите новый пароль"
					type="password"
					autocomplete="new-password"
					bind:value={repeatPassword}
					error={passwordErrors.repeat}
				/>
				<div>
					<Button type="submit" disabled={passwordPending} aria-busy={passwordPending}>
						{#if passwordPending}<LoaderIcon class="animate-spin" />Сохраняем…{:else}Сохранить пароль{/if}
					</Button>
				</div>
			</form>
		</Card.Content>
	</Card.Root>

	<IdentitiesCard {user} />

	<Card.Root>
		<Card.Header>
			<Card.Title>Активные сессии</Card.Title>
			<Card.Description>Устройства, на которых выполнен вход. Незнакомую сессию лучше завершить.</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if sessionsError}
				<Alert.Root variant="destructive">
					<CircleAlertIcon />
					<Alert.Description>
						<p>{sessionsError}</p>
						<Button size="sm" variant="outline" class="mt-2" onclick={loadSessions}>Повторить</Button>
					</Alert.Description>
				</Alert.Root>
			{:else if sessions === null}
				<div class="grid gap-3" aria-busy="true">
					<Skeleton class="h-16 w-full" />
					<Skeleton class="h-16 w-full" />
				</div>
			{:else if sessions.length === 0}
				<p class="text-muted-foreground text-sm">Активных сессий нет.</p>
			{:else}
				<ul class="divide-y rounded-lg border">
					{#each sessions as item (item.id)}
						<li class="flex flex-col gap-3 p-4 sm:flex-row sm:items-center">
							<MonitorSmartphoneIcon class="text-muted-foreground hidden size-5 shrink-0 sm:block" />
							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-2">
									<span class="font-medium">{describeUserAgent(item.user_agent)}</span>
									{#if item.current}<Badge variant="secondary">текущая</Badge>{/if}
								</div>
								<div class="text-muted-foreground mt-1 text-xs leading-relaxed">
									{#if item.ip}IP {item.ip} · {/if}вход {formatDateTime(item.created_at)}<br />
									активность {formatDateTime(item.last_seen_at)}
								</div>
							</div>
							{#if !item.current}
								<Button
									size="sm"
									variant="outline"
									class="self-start sm:self-center"
									disabled={revoking === item.id}
									onclick={() => revoke(item.id)}
								>
									{#if revoking === item.id}<LoaderIcon class="animate-spin" />{/if}
									Завершить
								</Button>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</Card.Content>
	</Card.Root>
</div>
