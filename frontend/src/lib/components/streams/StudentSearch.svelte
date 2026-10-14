<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import UserPlusIcon from '@lucide/svelte/icons/user-plus';
	import UserCheckIcon from '@lucide/svelte/icons/user-check';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';

	type Candidate = Schema<'Candidate'>;
	type Member = Schema<'Member'>;

	let {
		streamId,
		members,
		onchange
	}: { streamId: string; members: Member[]; onchange: () => void } = $props();

	let search = $state('');
	let query = $state('');

	// Поиск с задержкой, чтобы не дёргать сервер на каждую букву.
	$effect(() => {
		const value = search.trim();
		const timer = setTimeout(() => (query = value), 300);
		return () => clearTimeout(timer);
	});

	const candidates = new Resource(() =>
		unwrap(
			api.GET('/api/streams/{stream_id}/candidates', {
				params: { path: { stream_id: streamId }, query: { search: query || undefined } }
			})
		)
	);

	// Перечитываем выдачу при новом запросе и при любом изменении состава потока
	// (исключение, возврат, добавление списком).
	$effect(() => {
		void query;
		void members;
		void candidates.reload();
	});

	let busy = $state<string | null>(null);

	async function add(candidate: Candidate): Promise<void> {
		busy = candidate.user_id;
		try {
			await unwrap(
				api.POST('/api/streams/{stream_id}/members', {
					params: { path: { stream_id: streamId } },
					body: { emails: [candidate.email] }
				})
			);
			toast.success(
				candidate.membership === 'removed'
					? `${candidate.display_name} снова в потоке`
					: `${candidate.display_name} добавлен(а) в поток`
			);
			onchange();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			busy = null;
		}
	}
</script>

<div class="grid gap-3">
	<div class="relative">
		<SearchIcon class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
		<Input
			type="search"
			bind:value={search}
			placeholder="Имя или email студента"
			class="pl-8"
			aria-label="Поиск среди зарегистрированных"
		/>
	</div>

	<LoadState resource={candidates}>
		{#snippet children(found)}
			{#if found.users.length === 0}
				<p class="text-muted-foreground text-sm">
					{query ? 'Никого не нашлось. Незарегистрированным отправьте пригласительную ссылку.' : 'Зарегистрированных пользователей пока нет.'}
				</p>
			{:else}
				<ul class="max-h-80 divide-y overflow-y-auto rounded-lg border" aria-label="Найденные пользователи">
					{#each found.users as candidate (candidate.user_id)}
						<li class="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2">
							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-2">
									<span class="font-medium break-words">{candidate.display_name}</span>
									{#if !candidate.email_verified}<Badge variant="destructive">email не подтверждён</Badge>{/if}
								</div>
								<div class="text-muted-foreground text-sm break-all">{candidate.email}</div>
							</div>
							{#if candidate.membership === 'member'}
								<Badge variant="secondary">в потоке</Badge>
							{:else}
								<Button
									size="sm"
									variant={candidate.membership === 'removed' ? 'outline' : 'default'}
									disabled={busy !== null}
									onclick={() => add(candidate)}
								>
									{#if busy === candidate.user_id}
										<LoaderIcon class="animate-spin" />
									{:else if candidate.membership === 'removed'}
										<UserCheckIcon />
									{:else}
										<UserPlusIcon />
									{/if}
									{candidate.membership === 'removed' ? 'Вернуть' : 'Добавить'}
								</Button>
							{/if}
						</li>
					{/each}
				</ul>
				{#if found.more}
					<p class="text-muted-foreground text-xs">Показаны первые {found.users.length} — уточните поиск.</p>
				{/if}
			{/if}
		{/snippet}
	</LoadState>
</div>
