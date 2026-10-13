<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import UsersIcon from '@lucide/svelte/icons/users';
	import LinkIcon from '@lucide/svelte/icons/link-2-off';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { toast } from 'svelte-sonner';
	import AuthCard from '$lib/components/app/AuthCard.svelte';
	import RegisterForm from '$lib/components/app/RegisterForm.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { Resource } from '$lib/resource.svelte';

	const token = $derived(page.params.token ?? '');
	/** Вернулись со входа: вступаем сразу, без второго нажатия. */
	const autojoin = $derived(page.url.searchParams.get('autojoin') === '1');

	const invite = new Resource(() =>
		unwrap(api.GET('/api/invites/{token}', { params: { path: { token } } }))
	);
	$effect(() => {
		void token;
		void invite.reload();
	});

	const returnPath = $derived(`/invite/${encodeURIComponent(token)}?autojoin=1`);
	const loginHref = $derived(`/login?next=${encodeURIComponent(returnPath)}`);
	const yandexHref = $derived(
		`/api/auth/oauth/yandex/start?${new URLSearchParams({ next: returnPath, invite: token }).toString()}`
	);

	let joining = $state(false);
	let registering = $state(false);
	let joinError = $state<string | null>(null);
	let autoTried = false;

	async function join(): Promise<void> {
		joining = true;
		joinError = null;
		try {
			const joined = await unwrap(api.POST('/api/invites/{token}/join', { params: { path: { token } } }));
			if (joined.already_member) toast.info(`Вы уже состоите в потоке «${joined.stream_name}»`);
			else toast.success(`Вы вступили в поток «${joined.stream_name}»`);
			await goto('/', { replaceState: true });
		} catch (error) {
			joinError = errorMessage(error);
		} finally {
			joining = false;
		}
	}

	$effect(() => {
		if (autojoin && session.user && invite.data && !autoTried && !registering) {
			autoTried = true;
			void join();
		}
	});
</script>

<svelte:head>
	<title>Приглашение в поток — Зачтено</title>
</svelte:head>

{#if invite.error}
	<AuthCard title="Ссылка недействительна">
		<div class="flex flex-col items-center gap-3 text-center">
			<LinkIcon class="text-muted-foreground size-10" />
			<p>
				{invite.status === 404
					? 'Приглашение не найдено: возможно, ссылку отозвали или скопировали не полностью. Попросите у преподавателя новую.'
					: invite.error}
			</p>
			{#if invite.status !== 404}
				<Button variant="outline" onclick={() => invite.reload()}>Повторить</Button>
			{/if}
			<Button href="/" variant="outline" class="w-full">На главную</Button>
		</div>
	</AuthCard>
{:else if !invite.data}
	<div class="text-muted-foreground flex items-center justify-center gap-2 py-20 text-sm" role="status">
		<LoaderIcon class="size-5 animate-spin" />Проверяем приглашение…
	</div>
{:else}
	{@const info = invite.data}
	{#snippet streamInfo()}
		<div class="bg-muted/60 grid gap-2 rounded-lg p-4">
			<p class="flex items-center gap-2 font-medium"><UsersIcon class="text-primary size-4" />{info.stream_name}</p>
			{#if info.courses.length > 0}
				<div class="flex flex-wrap gap-1.5">
					{#each info.courses as course (course)}
						<Badge variant="secondary">{course}</Badge>
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}

	{#if session.user && !registering}
		<AuthCard title="Приглашение в поток" description="Вступите в поток, чтобы видеть его тесты.">
			<div class="grid gap-4">
				{@render streamInfo()}
				{#if joinError}
					<Alert.Root variant="destructive" aria-live="polite">
						<CircleAlertIcon />
						<Alert.Description>{joinError}</Alert.Description>
					</Alert.Root>
				{/if}
				<p class="text-muted-foreground text-sm">Вы вошли как {session.user.display_name} ({session.user.email}).</p>
				<Button size="lg" onclick={join} disabled={joining}>
					{#if joining}<LoaderIcon class="animate-spin" />Вступаем…{:else}Вступить в поток{/if}
				</Button>
			</div>
		</AuthCard>
	{:else}
		<AuthCard title="Приглашение в поток" description="Зарегистрируйтесь — и сразу попадёте в поток.">
			<div class="mb-4">{@render streamInfo()}</div>
			<RegisterForm
				{loginHref}
				{yandexHref}
				submitLabel="Зарегистрироваться и вступить"
				onpending={(value) => (registering = value)}
				onregistered={async () => {
					await join();
					registering = false;
				}}
			/>
			{#snippet footer()}
				<p>
					Уже есть аккаунт?
					<a href={loginHref} class="text-primary font-medium underline-offset-3 hover:underline">Войти и вступить</a>
				</p>
			{/snippet}
		</AuthCard>
	{/if}
{/if}
