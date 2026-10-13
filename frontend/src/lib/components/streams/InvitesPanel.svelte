<script lang="ts">
	import LinkIcon from '@lucide/svelte/icons/link';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import { toast } from 'svelte-sonner';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import OneTimeLink from '$lib/components/app/OneTimeLink.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { count, formatDateTime } from '$lib/format';
	import type { Resource } from '$lib/resource.svelte';

	let { streamId, invites }: { streamId: string; invites: Resource<Schema<'InviteSummary'>[]> } = $props();

	let label = $state('');
	let pending = $state(false);
	let issuedUrl = $state<string | null>(null);

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		pending = true;
		try {
			const issued = await unwrap(
				api.POST('/api/streams/{stream_id}/invites', {
					params: { path: { stream_id: streamId } },
					body: { label: label.trim() }
				})
			);
			issuedUrl = issued.url;
			label = '';
			await invites.reload();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			pending = false;
		}
	}
</script>

<div class="grid gap-6">
	<form class="bg-card grid gap-2 rounded-xl border p-4" onsubmit={create}>
		<Label for="invite-label">Новая пригласительная ссылка</Label>
		<div class="flex flex-col gap-2 sm:flex-row">
			<Input
				id="invite-label"
				bind:value={label}
				placeholder="Метка, например «Чат группы» (необязательно)"
				class="min-w-0 sm:max-w-sm"
				maxlength={200}
			/>
			<Button type="submit" disabled={pending}>
				{#if pending}<LoaderIcon class="animate-spin" />{:else}<PlusIcon />{/if}
				Создать ссылку
			</Button>
		</div>
		<p class="text-muted-foreground text-xs">
			По ссылке студент вступает в поток. Метка помогает понять в журнале, откуда пришёл студент.
		</p>
	</form>

	{#if issuedUrl}
		<OneTimeLink url={issuedUrl} ondismiss={() => (issuedUrl = null)} />
	{/if}

	<LoadState resource={invites}>
		{#snippet children(list)}
			{#if list.length === 0}
				<EmptyState icon={LinkIcon} title="Ссылок пока нет" text="Создайте первую ссылку и отправьте её студентам." />
			{:else}
				<ul class="bg-card divide-y rounded-xl border">
					{#each list as invite (invite.id)}
						<li class="flex flex-col gap-1 p-4 sm:flex-row sm:items-center sm:gap-4">
							<div class="min-w-0 flex-1">
								<div class="font-medium break-words">{invite.label || 'Без метки'}</div>
								<div class="text-muted-foreground text-xs">создана {formatDateTime(invite.created_at)}</div>
							</div>
							<div class="text-muted-foreground flex gap-4 text-sm">
								<span>{count(invite.uses, ['переход', 'перехода', 'переходов'])}</span>
								<span>{count(invite.students, ['студент', 'студента', 'студентов'])}</span>
							</div>
						</li>
					{/each}
				</ul>
				<p class="text-muted-foreground text-xs">
					Полный адрес ссылки виден только при создании. Потеряли — создайте новую.
				</p>
			{/if}
		{/snippet}
	</LoadState>
</div>
