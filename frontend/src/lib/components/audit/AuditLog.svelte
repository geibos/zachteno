<script lang="ts">
	import ScrollTextIcon from '@lucide/svelte/icons/scroll-text';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import * as Table from '$lib/components/ui/table';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import MskDateTimeInput from '$lib/components/app/MskDateTimeInput.svelte';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import { formatDateTime } from '$lib/format';
	import { selectClass, shortId } from '$lib/components/antifraud/model';
	import { AUDIT_ACTIONS, actionLabel, detailsText, objectTypeLabel } from './actions';

	/** `courseId` задан — журнал только по курсу (для методиста-владельца). */
	let { courseId }: { courseId?: string } = $props();

	let action = $state('');
	let from = $state<string | null>(null);
	let to = $state<string | null>(null);

	const records = new Resource(() =>
		unwrap(
			api.GET('/api/audit', {
				params: {
					query: {
						course_id: courseId || undefined,
						action: action || undefined,
						from: from ?? undefined,
						to: to ?? undefined
					}
				}
			})
		)
	);

	// Перезагрузка при смене фильтров.
	$effect(() => {
		void [courseId, action, from, to];
		void records.reload();
	});

	const filtered = $derived(action !== '' || from !== null || to !== null);
	const periodError = $derived(from && to && from > to ? 'Начало периода позже конца.' : null);

	function reset(): void {
		action = '';
		from = null;
		to = null;
	}
</script>

<div class="grid grid-cols-1 gap-4">
	<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-end">
		<div class="grid gap-2">
			<Label for="audit-action">Действие</Label>
			<select id="audit-action" class={selectClass} bind:value={action}>
				<option value="">Все действия</option>
				{#each AUDIT_ACTIONS as [code, label] (code)}
					<option value={code}>{label}</option>
				{/each}
			</select>
		</div>
		<MskDateTimeInput id="audit-from" label="С" bind:value={from} />
		<MskDateTimeInput id="audit-to" label="По" bind:value={to} />
		<Button variant="outline" onclick={reset} disabled={!filtered}>
			<RotateCcwIcon />Сбросить
		</Button>
	</div>
	{#if periodError}<p class="text-destructive text-sm">{periodError}</p>{/if}

	<LoadState resource={records}>
		{#snippet children(list)}
			{#if list.length === 0}
				<EmptyState
					icon={ScrollTextIcon}
					title="Записей нет"
					text={filtered ? 'Попробуйте изменить фильтры.' : 'Журнал пока пуст.'}
				/>
			{:else}
				<p class="text-muted-foreground text-sm" aria-live="polite">
					Записей: {list.length}{records.loading ? ' · обновляем…' : ''}
				</p>

				<!-- Телефон: карточки -->
				<ul class="grid gap-3 sm:hidden">
					{#each list as record (record.id)}
						{@const details = detailsText(record.details)}
						<li class="grid gap-1 rounded-lg border p-3 text-sm">
							<div class="flex flex-wrap items-baseline justify-between gap-x-2">
								<span class="font-medium">{actionLabel(record.action)}</span>
								<span class="text-muted-foreground text-xs">{formatDateTime(record.at)}</span>
							</div>
							<p class="text-muted-foreground text-xs break-words">
								{record.actor_name ?? 'система'}
								· {objectTypeLabel(record.object_type)}{#if record.object_id}&nbsp;<span class="font-mono">{shortId(record.object_id)}</span>{/if}
								{#if record.ip}· IP {record.ip}{/if}
							</p>
							{#if details}
								<details class="text-xs">
									<summary class="text-primary cursor-pointer">Подробности</summary>
									<pre class="bg-muted mt-1 overflow-x-auto rounded p-2 break-words whitespace-pre-wrap">{details}</pre>
								</details>
							{/if}
						</li>
					{/each}
				</ul>

				<!-- Планшет и шире: таблица -->
				<div class="hidden overflow-x-auto rounded-lg border sm:block">
					<Table.Root>
						<Table.Header>
							<Table.Row>
								<Table.Head>Когда (МСК)</Table.Head>
								<Table.Head>Кто</Table.Head>
								<Table.Head>Действие</Table.Head>
								<Table.Head>Объект</Table.Head>
								<Table.Head>IP</Table.Head>
							</Table.Row>
						</Table.Header>
						<Table.Body>
							{#each list as record (record.id)}
								{@const details = detailsText(record.details)}
								<Table.Row class="align-top">
									<Table.Cell class="text-xs whitespace-nowrap">{formatDateTime(record.at)}</Table.Cell>
									<Table.Cell>{record.actor_name ?? 'система'}</Table.Cell>
									<Table.Cell class="whitespace-normal">
										<span title={record.action}>{actionLabel(record.action)}</span>
										{#if details}
											<details class="mt-1 text-xs">
												<summary class="text-primary cursor-pointer">Подробности</summary>
												<pre class="bg-muted mt-1 max-w-md rounded p-2 break-words whitespace-pre-wrap">{details}</pre>
											</details>
										{/if}
									</Table.Cell>
									<Table.Cell class="text-xs">
										{objectTypeLabel(record.object_type)}
										{#if record.object_id}<span class="text-muted-foreground font-mono">{shortId(record.object_id)}</span>{/if}
									</Table.Cell>
									<Table.Cell class="text-muted-foreground font-mono text-xs">{record.ip ?? '—'}</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				</div>
			{/if}
		{/snippet}
	</LoadState>
</div>
