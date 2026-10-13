<script lang="ts">
	import type { Snippet } from 'svelte';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { toast } from 'svelte-sonner';
	import FormField from './FormField.svelte';
	import SubmitButton from './SubmitButton.svelte';
	import YandexButton from './YandexButton.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Separator } from '$lib/components/ui/separator';
	import { ApiError, api, errorMessage, unwrap, type CurrentUser } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { isEmailField, sendSignals, TypingRecorder, typingRhythm } from '$lib/signals';
	import {
		PASSWORD_MIN,
		validateEmail,
		validateName,
		validateNewPassword,
		validateRepeat
	} from '$lib/validation';

	/** Форма регистрации: страница /register и страница приглашения. */
	let {
		trusted = null,
		loginHref,
		yandexHref,
		yandexLabel = 'Продолжить с Яндекс ID',
		submitLabel = 'Зарегистрироваться',
		banner,
		onpending,
		onyandex,
		onregistered
	}: {
		trusted?: string | null;
		loginHref: string;
		yandexHref: string;
		yandexLabel?: string;
		submitLabel?: string;
		banner?: Snippet;
		/** Идёт ли отправка — чтобы страница не реагировала на вход раньше времени. */
		onpending?: (pending: boolean) => void;
		/** Перед уходом на Яндекс (например, запомнить намерение вступить). */
		onyandex?: () => void;
		/** Вызывается после создания учётной записи (сессия уже открыта). */
		onregistered: (user: CurrentUser) => Promise<void>;
	} = $props();

	// Ритм набора — только в поле email; пароль не наблюдается никогда.
	const typing = new TypingRecorder();

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

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		formError = null;
		emailTaken = false;
		if (Object.values(errors).some(Boolean)) return;
		pending = true;
		onpending?.(true);
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
			sendSignals('register', typing.snapshot());
			toast.success('Учётная запись создана', {
				description: `Мы отправили письмо для подтверждения на ${user.email}.`
			});
			await onregistered(user);
		} catch (error) {
			emailTaken = error instanceof ApiError && error.status === 409;
			formError = errorMessage(error);
		} finally {
			pending = false;
			onpending?.(false);
		}
	}
</script>

<form class="grid gap-4" novalidate onsubmit={submit} use:typingRhythm={{ recorder: typing, accept: isEmailField }}>
	{@render banner?.()}

	{#if formError}
		<Alert.Root variant="destructive" aria-live="polite">
			<CircleAlertIcon />
			<Alert.Description>
				<p>{formError}</p>
				{#if emailTaken}
					<p class="mt-1">
						<a href={loginHref} class="underline underline-offset-3">Войти</a>
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

	<SubmitButton {pending} pendingText="Создаём учётную запись…">{submitLabel}</SubmitButton>
</form>

{#if session.yandexEnabled}
	<div class="my-5 flex items-center gap-3">
		<Separator class="flex-1" />
		<span class="text-muted-foreground text-xs">или</span>
		<Separator class="flex-1" />
	</div>
	<YandexButton href={yandexHref} label={yandexLabel} onclick={onyandex} />
{/if}
