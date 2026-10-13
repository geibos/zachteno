<script lang="ts">
	import UserXIcon from '@lucide/svelte/icons/user-x';
	import UsersIcon from '@lucide/svelte/icons/users';
	import { toast } from 'svelte-sonner';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { api, unwrap, type Schema } from '$lib/api/client';
	import { formatDateTime } from '$lib/format';

	type Member = Schema<'Member'>;

	let {
		streamId,
		members,
		onchange
	}: { streamId: string; members: Member[]; onchange: () => void } = $props();

	const active = $derived(members.filter((member) => !member.removed_at));
	const removed = $derived(members.filter((member) => member.removed_at));

	let target = $state<Member | null>(null);
	let confirmOpen = $state(false);

	function askRemove(member: Member): void {
		target = member;
		confirmOpen = true;
	}

	async function remove(): Promise<void> {
		if (!target) return;
		const member = target;
		await unwrap(
			api.DELETE('/api/streams/{stream_id}/members/{user_id}', {
				params: { path: { stream_id: streamId, user_id: member.user_id } }
			})
		);
		toast.success(`${member.display_name} исключён(а) из потока`);
		onchange();
	}
</script>

{#snippet memberRow(member: Member, isRemoved: boolean)}
	<li class="flex flex-col gap-3 p-4 sm:flex-row sm:items-center {isRemoved ? 'opacity-60' : ''}">
		<div class="min-w-0 flex-1">
			<div class="flex flex-wrap items-center gap-2">
				<span class="font-medium break-words">{member.display_name}</span>
				{#if !member.email_verified}<Badge variant="destructive">email не подтверждён</Badge>{/if}
				{#if member.trusted_registration}<Badge variant="outline">доверенная регистрация</Badge>{/if}
			</div>
			<div class="text-muted-foreground mt-0.5 text-sm break-all">{member.email}</div>
			<div class="text-muted-foreground mt-1 text-xs">
				вступил(а) {formatDateTime(member.joined_at)}
				{#if member.removed_at}· исключён(а) {formatDateTime(member.removed_at)}{/if}
			</div>
		</div>
		{#if !isRemoved}
			<Button size="sm" variant="outline" class="self-start sm:self-center" onclick={() => askRemove(member)}>
				<UserXIcon />Исключить
			</Button>
		{/if}
	</li>
{/snippet}

<div class="grid gap-6">
	{#if active.length === 0}
		<EmptyState
			icon={UsersIcon}
			title="В потоке пока нет студентов"
			text="Создайте пригласительную ссылку на вкладке «Ссылки» и отправьте её студентам."
		/>
	{:else}
		<ul class="bg-card divide-y rounded-xl border">
			{#each active as member (member.user_id)}
				{@render memberRow(member, false)}
			{/each}
		</ul>
	{/if}

	{#if removed.length > 0}
		<section class="grid gap-2">
			<h2 class="text-muted-foreground text-sm font-medium">Исключённые ({removed.length})</h2>
			<ul class="divide-y rounded-xl border border-dashed">
				{#each removed as member (member.user_id)}
					{@render memberRow(member, true)}
				{/each}
			</ul>
		</section>
	{/if}
</div>

<ConfirmDialog
	bind:open={confirmOpen}
	title="Исключить из потока?"
	description={target
		? `${target.display_name} (${target.email}) потеряет доступ к тестам курсов этого потока. Уже сданные попытки и результаты сохранятся.`
		: undefined}
	confirmLabel="Исключить"
	destructive
	onconfirm={remove}
/>
