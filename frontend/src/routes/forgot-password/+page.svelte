<script lang="ts">
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import MailCheckIcon from '@lucide/svelte/icons/mail-check';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import FormField from '$lib/components/app/FormField.svelte';
	import SubmitButton from '$lib/components/app/SubmitButton.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { validateEmail } from '$lib/validation';

	let email = $state(session.user?.email ?? '');
	let submitted = $state(false);
	let pending = $state(false);
	let sentTo = $state<string | null>(null);
	let formError = $state<string | null>(null);

	const emailError = $derived(submitted ? validateEmail(email) : null);

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		formError = null;
		if (emailError) return;
		pending = true;
		try {
			await unwrap(api.POST('/api/auth/password/forgot', { body: { email: email.trim() } }));
			sentTo = email.trim();
		} catch (error) {
			formError = errorMessage(error);
		} finally {
			pending = false;
		}
	}
</script>

<svelte:head>
	<title>Восстановление пароля — Зачтено</title>
</svelte:head>

{#if sentTo}
	<AuthCard title="Проверьте почту">
		<div class="flex flex-col items-center gap-3 text-center">
			<MailCheckIcon class="text-primary size-10" />
			<p>
				Если адрес <span class="font-medium break-all">{sentTo}</span> зарегистрирован, мы отправили на
				него ссылку для восстановления пароля.
			</p>
			<p class="text-muted-foreground text-sm">
				Ссылка действует 1 час. Письма нет — проверьте «Спам» или попробуйте ещё раз.
			</p>
			<div class="mt-2 flex w-full flex-col gap-2 sm:flex-row">
				<Button variant="outline" class="flex-1" onclick={() => (sentTo = null)}>Отправить ещё раз</Button>
				<Button href="/login" class="flex-1">Ко входу</Button>
			</div>
		</div>
	</AuthCard>
{:else}
	<AuthCard
		title="Восстановление пароля"
		description="Укажите email учётной записи — пришлём ссылку для смены пароля."
	>
		<form class="grid gap-4" novalidate onsubmit={submit}>
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
				error={emailError}
			/>
			<SubmitButton {pending} pendingText="Отправляем…">Отправить ссылку</SubmitButton>
		</form>

		{#snippet footer()}
			<p>
				Вспомнили пароль?
				<a href="/login" class="text-primary font-medium underline-offset-3 hover:underline">Войти</a>
			</p>
		{/snippet}
	</AuthCard>
{/if}
