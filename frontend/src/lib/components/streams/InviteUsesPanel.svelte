<script lang="ts">
	import HistoryIcon from '@lucide/svelte/icons/history';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import * as Select from '$lib/components/ui/select';
	import * as Table from '$lib/components/ui/table';
	import { Label } from '$lib/components/ui/label';
	import { Button } from '$lib/components/ui/button';
	import { api, unwrap, type Schema } from '$lib/api/client';
	import { formatDateTime } from '$lib/format';
	import { Resource } from '$lib/resource.svelte';

	const ALL = 'all';

	let {
		streamId,
		invites,
		members
	}: { streamId: string; invites: Schema<'InviteSummary'>[]; members: Schema<'Member'>[] } = $props();

	let linkId = $state(ALL);
	let userId = $state(ALL);

	const uses = new Resource(() =>
		unwrap(
			api.GET('/api/streams/{stream_id}/invite-uses', {
				params: {
					path: { stream_id: streamId },
					query: {
						link_id: linkId === ALL ? undefined : linkId,
						user_id: userId === ALL ? undefined : userId
					}
				}
			})
		)
	);

	$effect(() => {
		// Перезагрузка при смене фильтров.
		void linkId;
		void userId;
		void uses.reload();
	});

	const linkLabel = $derived(
		linkId === ALL ? 'Все ссылки' : (invites.find((invite) => invite.id === linkId)?.label || 'Без метки')
	);
	const userLabel = $derived(
		userId === ALL ? 'Все студенты' : (members.find((member) => member.user_id === userId)?.display_name ?? 'Студент')
	);
</script>

<div class="grid gap-4">
	<div class="grid gap-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end">
		<div class="grid min-w-0 gap-2">
			<Label for="filter-link">Ссылка</Label>
			<Select.Root type="single" bind:value={linkId}>
				<Select.Trigger id="filter-link" class="w-full min-w-0">
					<span class="truncate">{linkLabel}</span>
				</Select.Trigger>
				<Select.Content>
					<Select.Item value={ALL} label="Все ссылки">Все ссылки</Select.Item>
					{#each invites as invite (invite.id)}
						<Select.Item value={invite.id} label={invite.label || 'Без метки'}>
							{invite.label || 'Без метки'} · {formatDateTime(invite.created_at)}
						</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
		</div>
		<div class="grid min-w-0 gap-2">
			<Label for="filter-user">Студент</Label>
			<Select.Root type="single" bind:value={userId}>
				<Select.Trigger id="filter-user" class="w-full min-w-0">
					<span class="truncate">{userLabel}</span>
				</Select.Trigger>
				<Select.Content>
					<Select.Item value={ALL} label="Все студенты">Все студенты</Select.Item>
					{#each members as member (member.user_id)}
						<Select.Item value={member.user_id} label={member.display_name}>
							{member.display_name} · {member.email}
						</Select.Item>
					{/each}
				</Select.Content>
			</Select.Root>
		</div>
		{#if linkId !== ALL || userId !== ALL}
			<Button
				variant="ghost"
				onclick={() => {
					linkId = ALL;
					userId = ALL;
				}}
			>
				Сбросить
			</Button>
		{/if}
	</div>

	<LoadState resource={uses}>
		{#snippet children(list)}
			{#if list.length === 0}
				<EmptyState icon={HistoryIcon} title="Переходов нет" text="Здесь появится, кто и когда перешёл по пригласительной ссылке." />
			{:else}
				<!-- Мобильный вид: карточки -->
				<ul class="bg-card divide-y rounded-xl border sm:hidden">
					{#each list as use, index (index)}
						<li class="grid gap-0.5 p-4 text-sm">
							<span class="font-medium break-words">{use.display_name}</span>
							<span class="text-muted-foreground break-all">{use.email}</span>
							<span class="text-muted-foreground text-xs">
								{formatDateTime(use.used_at)} · {use.link_label || 'Без метки'}
							</span>
						</li>
					{/each}
				</ul>
				<!-- sm+: таблица -->
				<div class="bg-card hidden overflow-x-auto rounded-xl border sm:block">
					<Table.Root>
						<Table.Header>
							<Table.Row>
								<Table.Head>Когда</Table.Head>
								<Table.Head>Студент</Table.Head>
								<Table.Head>Ссылка</Table.Head>
							</Table.Row>
						</Table.Header>
						<Table.Body>
							{#each list as use, index (index)}
								<Table.Row>
									<Table.Cell class="whitespace-nowrap">{formatDateTime(use.used_at)}</Table.Cell>
									<Table.Cell>
										<div class="font-medium">{use.display_name}</div>
										<div class="text-muted-foreground text-xs">{use.email}</div>
									</Table.Cell>
									<Table.Cell>{use.link_label || 'Без метки'}</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				</div>
			{/if}
		{/snippet}
	</LoadState>
</div>
