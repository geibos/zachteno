<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Alert from '$lib/components/ui/alert';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { formatScore } from '$lib/format';
	import { cn } from '$lib/utils';
	import { eventLabel, formatMskSeconds, isNotableEvent } from './table';

	/** Детали попытки: ответы по вопросам и хронология событий. `attemptId = null` — закрыто. */
	let {
		attemptId = $bindable(null),
		columns
	}: { attemptId?: string | null; columns: Schema<'ColumnHeader'>[] } = $props();

	let details = $state<Schema<'AttemptDetails'> | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);
	let generation = 0;

	const numberById = $derived(new Map(columns.map((column) => [column.question_id, column.number])));

	$effect(() => {
		const id = attemptId;
		if (!id) return;
		const current = ++generation;
		details = null;
		error = null;
		loading = true;
		unwrap(api.GET('/api/attempts/{attempt_id}/details', { params: { path: { attempt_id: id } } }))
			.then((data) => {
				if (current === generation) details = data;
			})
			.catch((err: unknown) => {
				if (current === generation) error = errorMessage(err);
			})
			.finally(() => {
				if (current === generation) loading = false;
			});
	});

	const open = $derived(attemptId !== null);
</script>

<Dialog.Root
	{open}
	onOpenChange={(value) => {
		if (!value) attemptId = null;
	}}
>
	<Dialog.Content class="max-h-[90dvh] overflow-y-auto sm:max-w-2xl">
		<Dialog.Header>
			<Dialog.Title>{details ? details.display_name : 'Детали попытки'}</Dialog.Title>
			{#if details}<Dialog.Description>{details.email}</Dialog.Description>{/if}
		</Dialog.Header>

		{#if loading}
			<div class="text-muted-foreground flex items-center gap-2 py-6 text-sm" role="status">
				<LoaderIcon class="size-4 animate-spin" />Загрузка…
			</div>
		{:else if error}
			<Alert.Root variant="destructive">
				<CircleAlertIcon />
				<Alert.Description>{error}</Alert.Description>
			</Alert.Root>
		{:else if details}
			<section class="grid gap-2">
				<h3 class="text-sm font-semibold">Ответы</h3>
				<ul class="divide-y rounded-lg border text-sm">
					{#each details.cells as cell (cell.question_id)}
						<li class="grid grid-cols-[3rem_1fr_auto] items-start gap-2 px-3 py-2">
							<span class="text-muted-foreground font-medium">В{numberById.get(cell.question_id) ?? '?'}</span>
							<span class="min-w-0 break-words whitespace-pre-wrap">
								{#if !cell.in_attempt}
									<span class="text-muted-foreground">вопроса не было в попытке</span>
								{:else if cell.free_text}
									{cell.text_answer?.trim() ? cell.text_answer : '—'}
								{:else}
									{cell.selected.length > 0 ? cell.selected.join('; ') : '—'}
								{/if}
							</span>
							<span class="text-right whitespace-nowrap tabular-nums">
								{#if cell.in_attempt}
									{cell.free_text ? 'не оценивается' : formatScore(cell.score)}
								{/if}
							</span>
						</li>
					{/each}
				</ul>
			</section>

			<section class="grid gap-2">
				<h3 class="text-sm font-semibold">Хронология (МСК)</h3>
				{#if details.events.length === 0}
					<p class="text-muted-foreground text-sm">Событий нет.</p>
				{:else}
					<ol class="relative grid gap-2 border-l pl-4 text-sm">
						{#each details.events as event, index (index)}
							<li class="relative">
								<span
									class={cn(
										'absolute top-1.5 -left-[1.3rem] size-2.5 rounded-full border-2 border-background',
										isNotableEvent(event.kind) ? 'bg-amber-500' : 'bg-primary/60'
									)}
									aria-hidden="true"
								></span>
								<span class="text-muted-foreground mr-2 text-xs tabular-nums">{formatMskSeconds(event.at)}</span>
								<span class={cn(isNotableEvent(event.kind) && 'text-amber-700 dark:text-amber-300')}>
									{eventLabel(event, (id) => numberById.get(id))}
								</span>
							</li>
						{/each}
					</ol>
				{/if}
			</section>
		{/if}
	</Dialog.Content>
</Dialog.Root>
