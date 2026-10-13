<script lang="ts">
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import SortHeader from './SortHeader.svelte';
	import { formatDateTime, formatScore } from '$lib/format';
	import { submitReasonText } from '$lib/attempt/review-status';
	import { cn } from '$lib/utils';
	import {
		formatSeconds,
		nextSort,
		shorten,
		sortRows,
		type Mode,
		type ResultsCell,
		type SortDir,
		type SortKey
	} from './table';
	import type { Schema } from '$lib/api/client';

	let {
		table,
		mode,
		onopen
	}: { table: Schema<'ResultsTable'>; mode: Mode; onopen: (attemptId: string) => void } = $props();

	let sort = $state<{ key: SortKey; dir: SortDir }>({ key: 'student', dir: 'asc' });
	const rows = $derived(sortRows(table.rows, sort.key, sort.dir));

	/** Раскрытые ячейки: `${attempt_id}:${question_id}`. */
	let expanded = $state<Record<string, boolean>>({});

	function onsort(key: SortKey): void {
		sort = nextSort(sort, key);
	}

	function cellOf(cells: ResultsCell[], questionId: string): ResultsCell | undefined {
		return cells.find((cell) => cell.question_id === questionId);
	}

	function hasContent(cell: ResultsCell): boolean {
		return cell.in_attempt && (cell.selected.length > 0 || Boolean(cell.text_answer?.trim()));
	}

	function scoreTone(cell: ResultsCell, weight: string): string {
		if (cell.free_text || cell.score === null || cell.score === undefined) return '';
		const score = Number(cell.score);
		const max = Number(weight);
		if (score >= max && max > 0) return 'text-emerald-700 dark:text-emerald-400';
		if (score <= 0) return 'text-destructive';
		return 'text-amber-700 dark:text-amber-400';
	}
</script>

<div class="overflow-x-auto rounded-lg border">
	<table class="w-full border-separate border-spacing-0 text-sm">
		<thead>
			<tr>
				<SortHeader sortKey="student" {sort} {onsort} class="bg-muted sticky left-0 z-20 min-w-44">Студент</SortHeader>
				<SortHeader sortKey="score" {sort} {onsort}>Балл</SortHeader>
				<SortHeader sortKey="submitted" {sort} {onsort}>Сдана</SortHeader>
				<SortHeader sortKey="duration" {sort} {onsort}>Время</SortHeader>
				{#each table.columns as column (column.question_id)}
					<SortHeader sortKey="q:{column.question_id}" {sort} {onsort} title={shorten(column.body)}>
						В{column.number}
					</SortHeader>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.attempt_id)}
				<tr class={cn('group', row.removed && 'text-muted-foreground')}>
					<th
						scope="row"
						class="bg-background group-hover:bg-muted sticky left-0 z-10 max-w-56 border-b px-3 py-2 text-left align-top font-normal"
					>
						<div class="flex items-start gap-1.5">
							<div class="min-w-0">
								<div class="truncate font-medium" title={row.display_name}>{row.display_name}</div>
								<div class="text-muted-foreground truncate text-xs" title={row.email}>{row.email}</div>
							</div>
							{#if row.antifraud}
								<span title="Есть сигналы антифрода" class="text-amber-600 dark:text-amber-400">
									<ShieldAlertIcon class="size-4" aria-label="Есть сигналы антифрода" />
								</span>
							{/if}
						</div>
						<div class="mt-1 flex flex-wrap items-center gap-1">
							{#if row.removed}<Badge variant="outline">исключён</Badge>{/if}
							{#if mode === 'all'}<Badge variant="secondary">попытка {row.attempt_number}</Badge>{/if}
							{#if row.stream_name}<span class="text-muted-foreground text-xs">{row.stream_name}</span>{/if}
						</div>
						<Button
							variant="link"
							size="xs"
							class="mt-0.5 h-auto px-0"
							onclick={() => onopen(row.attempt_id)}
						>
							Подробнее
						</Button>
					</th>
					<td class="group-hover:bg-muted/50 border-b px-3 py-2 align-top whitespace-nowrap tabular-nums">
						<span class="font-medium">{formatScore(row.score)}</span>
						<span class="text-muted-foreground">/ {formatScore(row.max_score)}</span>
						{#if mode === 'all' && row.final_score !== null && row.final_score !== undefined}
							<div class="text-muted-foreground text-xs">итог {formatScore(row.final_score)}</div>
						{/if}
					</td>
					<td class="group-hover:bg-muted/50 border-b px-3 py-2 align-top text-xs whitespace-nowrap">
						{formatDateTime(row.submitted_at)}
						{#if submitReasonText(row.submit_reason)}
							<div class="text-muted-foreground">{submitReasonText(row.submit_reason)}</div>
						{/if}
					</td>
					<td class="group-hover:bg-muted/50 border-b px-3 py-2 align-top whitespace-nowrap tabular-nums">
						{formatSeconds(row.duration_seconds)}
					</td>
					{#each table.columns as column (column.question_id)}
						{@const cell = cellOf(row.cells, column.question_id)}
						{@const key = `${row.attempt_id}:${column.question_id}`}
						<td class="group-hover:bg-muted/50 border-b px-3 py-2 align-top">
							{#if !cell || !cell.in_attempt}
								<span class="text-muted-foreground" title="Вопроса не было в этой попытке">—</span>
							{:else}
								<button
									type="button"
									class="hover:bg-muted focus-visible:ring-ring/50 -mx-1 inline-flex items-center gap-0.5 rounded px-1 text-left whitespace-nowrap outline-none focus-visible:ring-3 disabled:cursor-default"
									aria-expanded={expanded[key] ?? false}
									disabled={!hasContent(cell)}
									title={hasContent(cell) ? 'Показать ответ' : 'Нет ответа'}
									onclick={() => (expanded[key] = !expanded[key])}
								>
									{#if cell.free_text}
										<span class="text-muted-foreground text-xs">не оценивается</span>
									{:else}
										<span class={cn('font-medium tabular-nums', scoreTone(cell, column.weight))}>
											{formatScore(cell.score)}
										</span>
									{/if}
									{#if hasContent(cell)}
										<ChevronDownIcon class={cn('size-3 transition-transform', expanded[key] && 'rotate-180')} />
									{/if}
								</button>
								{#if !hasContent(cell)}
									<div class="text-muted-foreground text-xs">нет ответа</div>
								{/if}
								{#if expanded[key]}
									<div class="bg-muted/60 mt-1 w-48 rounded p-2 text-xs break-words whitespace-pre-wrap">
										{cell.free_text ? cell.text_answer : cell.selected.join('; ')}
									</div>
								{/if}
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>
