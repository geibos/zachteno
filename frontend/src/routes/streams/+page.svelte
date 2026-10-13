<script lang="ts">
	import { goto } from '$app/navigation';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import UsersIcon from '@lucide/svelte/icons/users';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import FormField from '$lib/components/app/FormField.svelte';
	import * as Card from '$lib/components/ui/card';
	import * as Alert from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { count, formatDateTime } from '$lib/format';
	import { resource } from '$lib/resource.svelte';

	const streams = resource(() => unwrap(api.GET('/api/streams')));

	let creating = $state(false);
	let name = $state('');
	let submitted = $state(false);
	let pending = $state(false);
	let formError = $state<string | null>(null);

	const nameError = $derived(submitted && name.trim() === '' ? 'Укажите название потока.' : null);

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		submitted = true;
		formError = null;
		if (nameError) return;
		pending = true;
		try {
			const created = await unwrap(api.POST('/api/streams', { body: { name: name.trim() } }));
			await goto(`/streams/${created.id}`);
		} catch (error) {
			formError = errorMessage(error);
		} finally {
			pending = false;
		}
	}

	function cancel(): void {
		creating = false;
		name = '';
		submitted = false;
		formError = null;
	}
</script>

<svelte:head>
	<title>Потоки — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title="Потоки"
				description="Группы студентов: участники, назначенные курсы и пригласительные ссылки."
			>
				{#snippet actions()}
					{#if session.isStaff && !creating}
						<Button onclick={() => (creating = true)}><PlusIcon />Создать поток</Button>
					{/if}
				{/snippet}
			</PageHeader>

			{#if creating}
				<Card.Root>
					<Card.Header>
						<Card.Title>Новый поток</Card.Title>
						<Card.Description>Например, «ПИ-21, осень 2026». Название видят студенты на странице приглашения.</Card.Description>
					</Card.Header>
					<Card.Content>
						<form class="grid max-w-md gap-4" novalidate onsubmit={create}>
							{#if formError}
								<Alert.Root variant="destructive" aria-live="polite">
									<CircleAlertIcon />
									<Alert.Description>{formError}</Alert.Description>
								</Alert.Root>
							{/if}
							<FormField id="stream-name" label="Название" bind:value={name} error={nameError} />
							<div class="flex flex-wrap gap-2">
								<Button type="submit" disabled={pending}>
									{#if pending}<LoaderIcon class="animate-spin" />{/if}
									Создать
								</Button>
								<Button variant="outline" onclick={cancel} disabled={pending}>Отмена</Button>
							</div>
						</form>
					</Card.Content>
				</Card.Root>
			{/if}

			<LoadState resource={streams}>
				{#snippet children(list)}
					{#if list.length === 0}
						<EmptyState
							icon={UsersIcon}
							title="Потоков пока нет"
							text={session.isStaff
								? 'Создайте поток, назначьте ему курс и раздайте студентам пригласительную ссылку.'
								: 'Здесь появятся потоки курсов, которые вы ведёте.'}
						/>
					{:else}
						<ul class="grid gap-3 sm:grid-cols-2">
							{#each list as stream (stream.id)}
								<li>
									<a
										href="/streams/{stream.id}"
										class="bg-card hover:border-primary/40 focus-visible:ring-ring/50 flex h-full flex-col gap-2 rounded-xl border p-4 transition-colors outline-none focus-visible:ring-3"
									>
										<span class="font-medium break-words">{stream.name}</span>
										<span class="text-muted-foreground text-sm">
											{count(stream.members, ['участник', 'участника', 'участников'])} · создан {formatDateTime(stream.created_at)}
										</span>
										{#if stream.courses.length > 0}
											<span class="flex flex-wrap gap-1.5">
												{#each stream.courses as course, index (index)}
													<Badge variant="secondary" class="max-w-full truncate">{course}</Badge>
												{/each}
											</span>
										{:else}
											<span class="text-muted-foreground text-xs">Курсы не назначены</span>
										{/if}
									</a>
								</li>
							{/each}
						</ul>
					{/if}
				{/snippet}
			</LoadState>
		</div>
	{/snippet}
</RequireAuth>
