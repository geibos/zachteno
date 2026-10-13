<script lang="ts">
	import InfoIcon from '@lucide/svelte/icons/info';
	import UsersIcon from '@lucide/svelte/icons/users';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import * as Alert from '$lib/components/ui/alert';
	import * as Card from '$lib/components/ui/card';
	import { Label } from '$lib/components/ui/label';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LinkCard from './LinkCard.svelte';
	import DecisionDialog from './DecisionDialog.svelte';
	import { api, unwrap } from '$lib/api/client';
	import { resource } from '$lib/resource.svelte';
	import { count } from '$lib/format';
	import {
		filterLinks,
		linkKey,
		selectClass,
		shortId,
		sortLinks,
		usersById,
		type DecisionFilter,
		type LevelFilter,
		type LinkView
	} from './model';

	let { courseId }: { courseId: string } = $props();

	const data = resource(() =>
		unwrap(api.GET('/api/courses/{course_id}/antifraud', { params: { path: { course_id: courseId } } }))
	);

	let level = $state<LevelFilter>('all');
	let decision = $state<DecisionFilter>('all');

	let dialogOpen = $state(false);
	let dialogLink = $state<LinkView | null>(null);
	let dialogDecision = $state<'confirmed' | 'rejected'>('confirmed');

	function decide(link: LinkView, value: 'confirmed' | 'rejected'): void {
		dialogLink = link;
		dialogDecision = value;
		dialogOpen = true;
	}
</script>

<div class="mx-auto grid w-full max-w-5xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
	<PageHeader
		title="Связанные аккаунты"
		description="Возможные совместные прохождения по сигналам браузеров, сетей и ответов."
		back={{ href: `/courses/${courseId}`, label: 'К курсу' }}
	/>

	<Alert.Root>
		<InfoIcon />
		<Alert.Title>Это только сигнал</Alert.Title>
		<Alert.Description>
			Ничего не блокируется и не влияет на баллы. Между разными устройствами без общей сети или похожих
			ответов связь может не найтись.
		</Alert.Description>
	</Alert.Root>

	<LoadState resource={data}>
		{#snippet children(result)}
			{@const users = usersById(result.links)}
			{@const shown = sortLinks(filterLinks(result.links, level, decision))}

			<section class="grid gap-3" aria-labelledby="clusters-heading">
				<h2 id="clusters-heading" class="flex items-center gap-2 text-lg font-semibold">
					<UsersIcon class="text-primary size-5" />Группы
				</h2>
				<p class="text-muted-foreground text-sm">
					Аккаунты, связанные на среднем уровне и выше; отклонённые связи не учитываются.
				</p>
				{#if result.clusters.length === 0}
					<p class="text-muted-foreground text-sm">Групп нет.</p>
				{:else}
					<div class="grid gap-3 sm:grid-cols-2">
						{#each result.clusters as cluster, i (i)}
							<Card.Root class="gap-3">
								<Card.Header>
									<Card.Title class="text-sm">
										Группа {i + 1} · {count(cluster.length, ['аккаунт', 'аккаунта', 'аккаунтов'])}
									</Card.Title>
								</Card.Header>
								<Card.Content>
									<ul class="grid gap-1.5 text-sm">
										{#each cluster as id (id)}
											{@const user = users.get(id)}
											<li class="min-w-0">
												{#if user}
													<span class="font-medium">{user.display_name}</span>
													<span class="text-muted-foreground block truncate text-xs">{user.email}</span>
												{:else}
													<span class="text-muted-foreground font-mono text-xs">{shortId(id)}…</span>
												{/if}
											</li>
										{/each}
									</ul>
								</Card.Content>
							</Card.Root>
						{/each}
					</div>
				{/if}
			</section>

			<section class="grid gap-3" aria-labelledby="links-heading">
				<h2 id="links-heading" class="flex items-center gap-2 text-lg font-semibold">
					<ShieldCheckIcon class="text-primary size-5" />Связи
				</h2>
				<div class="grid gap-3 sm:grid-cols-2">
					<div class="grid gap-1.5">
						<Label for="af-level">Уровень</Label>
						<select id="af-level" class={selectClass} bind:value={level}>
							<option value="all">Все уровни</option>
							<option value="high">Высокий</option>
							<option value="medium">Средний</option>
							<option value="low">Низкий</option>
						</select>
					</div>
					<div class="grid gap-1.5">
						<Label for="af-decision">Решение</Label>
						<select id="af-decision" class={selectClass} bind:value={decision}>
							<option value="all">Все</option>
							<option value="none">Без решения</option>
							<option value="confirmed">Подтверждено</option>
							<option value="rejected">Отклонено</option>
						</select>
					</div>
				</div>

				{#if shown.length === 0}
					<EmptyState
						title="Связей не найдено"
						text={result.links.length > 0 ? 'Попробуйте изменить фильтры.' : undefined}
					/>
				{:else}
					<p class="text-muted-foreground text-sm">
						Показано {shown.length} из {result.links.length}
					</p>
					<div class="grid gap-4">
						{#each shown as link (linkKey(link))}
							<LinkCard {link} ondecide={(value) => decide(link, value)} />
						{/each}
					</div>
				{/if}
			</section>
		{/snippet}
	</LoadState>
</div>

<DecisionDialog
	bind:open={dialogOpen}
	{courseId}
	link={dialogLink}
	decision={dialogDecision}
	ondone={() => void data.reload()}
/>
