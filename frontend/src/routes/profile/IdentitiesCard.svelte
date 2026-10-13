<script lang="ts">
	import LinkIcon from '@lucide/svelte/icons/link';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import * as Alert from '$lib/components/ui/alert';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import { ApiError, api, unwrap, type CurrentUser } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { resource } from '$lib/resource.svelte';
	import { formatDateTime } from '$lib/format';

	let { user }: { user: CurrentUser } = $props();

	const identities = resource(() => unwrap(api.GET('/api/profile/identities')));
	const yandex = $derived(identities.data?.find((identity) => identity.provider === 'yandex') ?? null);

	const linkHref = `/api/auth/oauth/yandex/start?${new URLSearchParams({ intent: 'link', next: '/profile' }).toString()}`;

	let confirmOpen = $state(false);
	let unlinkError = $state<string | null>(null);

	async function unlink(): Promise<void> {
		unlinkError = null;
		try {
			await unwrap(
				api.DELETE('/api/profile/identities/{provider}', { params: { path: { provider: 'yandex' } } })
			);
			toast.success('Яндекс ID отвязан');
			await identities.reload();
		} catch (error) {
			if (error instanceof ApiError && error.status === 409) {
				// Диалог закроем сами и покажем пояснение в карточке.
				unlinkError = 'Это единственный способ входа. Сначала задайте пароль в блоке выше, затем отвяжите Яндекс ID.';
				confirmOpen = false;
				return;
			}
			throw error;
		}
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title>Вход через Яндекс</Card.Title>
		<Card.Description>Привяжите Яндекс ID, чтобы входить без пароля.</Card.Description>
	</Card.Header>
	<Card.Content class="grid gap-3">
		{#if identities.error}
			<p class="text-destructive text-sm">{identities.error}</p>
		{:else if identities.data === null}
			<Skeleton class="h-10 w-full" />
		{:else if yandex}
			<div class="flex flex-col gap-3 rounded-lg border p-4 sm:flex-row sm:items-center">
				<svg viewBox="0 0 24 24" class="size-6 shrink-0" aria-hidden="true">
					<circle cx="12" cy="12" r="12" fill="#fc3f1d" />
					<text x="12" y="17" text-anchor="middle" font-size="14" font-weight="700" fill="#fff" font-family="system-ui, sans-serif">Я</text>
				</svg>
				<div class="min-w-0 flex-1">
					<p class="font-medium">Яндекс ID привязан</p>
					<p class="text-muted-foreground text-xs break-all">
						{yandex.email ?? 'email не передан'} · с {formatDateTime(yandex.created_at)}
					</p>
				</div>
				<Button variant="outline" size="sm" class="self-start sm:self-center" onclick={() => (confirmOpen = true)}>
					Отвязать
				</Button>
			</div>
			{#if unlinkError}
				<Alert.Root variant="destructive" aria-live="polite">
					<Alert.Description>{unlinkError}</Alert.Description>
				</Alert.Root>
			{/if}
		{:else if session.yandexEnabled}
			<p class="text-muted-foreground text-sm">Яндекс ID не привязан.</p>
			<Button href={linkHref} variant="outline" class="w-fit" data-sveltekit-reload>
				<LinkIcon />Привязать Яндекс ID
			</Button>
		{:else}
			<p class="text-muted-foreground text-sm">Вход через Яндекс на этом сервере не настроен.</p>
		{/if}
	</Card.Content>
</Card.Root>

<ConfirmDialog
	bind:open={confirmOpen}
	title="Отвязать Яндекс ID?"
	description={user.has_password
		? 'Входить можно будет по email и паролю.'
		: 'У вас нет пароля — без Яндекс ID войти будет нельзя. Сервер откажет, пока вы не зададите пароль.'}
	confirmLabel="Отвязать"
	destructive
	onconfirm={unlink}
/>
