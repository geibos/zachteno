<script lang="ts">
	import { page } from '$app/state';
	import UsersIcon from '@lucide/svelte/icons/users';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import ReportFilters from '$lib/components/reports/ReportFilters.svelte';
	import SortHeader from '$lib/components/reports/SortHeader.svelte';
	import {
		csvUrl,
		nextSort,
		sortSummaryRows,
		type Mode,
		type SortDir,
		type SortKey,
		type SummaryCell
	} from '$lib/components/reports/table';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import { count, formatPercent, formatScore } from '$lib/format';

	const courseId = $derived(page.params.id ?? '');

	let streamId = $state('');
	let mode = $state<Mode>('final');
	let sort = $state<{ key: SortKey; dir: SortDir }>({ key: 'student', dir: 'asc' });

	const query = $derived({ stream_id: streamId || undefined, mode });

	const summary = new Resource(() =>
		unwrap(api.GET('/api/courses/{course_id}/summary', { params: { path: { course_id: courseId }, query } }))
	);

	const course = new Resource(() =>
		unwrap(api.GET('/api/courses/{course_id}', { params: { path: { course_id: courseId } } }))
	);
	const courseStreams = new Resource(() =>
		unwrap(api.GET('/api/courses/{course_id}/streams', { params: { path: { course_id: courseId } } }))
	);

	$effect(() => {
		void courseId;
		void course.reload();
		void courseStreams.reload();
	});

	$effect(() => {
		void courseId;
		void query;
		void summary.reload();
	});

	const streams = $derived((courseStreams.data ?? []).map((stream) => ({ id: stream.stream_id, name: stream.name })));

	const csvHref = $derived(csvUrl(`/api/courses/${courseId}/summary.csv`, { stream_id: streamId, mode }));

	function cellOf(cells: SummaryCell[], testId: string): SummaryCell | undefined {
		return cells.find((cell) => cell.test_id === testId);
	}
</script>

<svelte:head>
	<title>Сводная таблица{course.data ? `: ${course.data.title}` : ''} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-6xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title={course.data ? `Сводная таблица: ${course.data.title}` : 'Сводная таблица курса'}
				description="Итоговые баллы студентов по всем тестам курса."
				back={{ href: `/courses/${courseId}`, label: 'К курсу' }}
			/>

			<ReportFilters {streams} bind:streamId bind:mode {csvHref} csvName="summary.csv" />

			<LoadState resource={summary}>
				{#snippet children(data)}
					{#if data.rows.length === 0 || data.tests.length === 0}
						<EmptyState icon={UsersIcon} title="Данных пока нет" text="Здесь появятся студенты потоков курса и их баллы." />
					{:else}
						{@const rows = sortSummaryRows(data.rows, sort.key, sort.dir)}
						<p class="text-muted-foreground text-sm">
							{count(data.rows.length, ['студент', 'студента', 'студентов'])} ×
							{count(data.tests.length, ['тест', 'теста', 'тестов'])}.
						</p>
						<div class="overflow-x-auto rounded-lg border">
							<table class="w-full border-separate border-spacing-0 text-sm">
								<thead>
									<tr>
										<SortHeader sortKey="student" {sort} onsort={(key) => (sort = nextSort(sort, key))} class="bg-muted sticky left-0 z-20 min-w-44">
											Студент
										</SortHeader>
										{#each data.tests as test (test.test_id)}
											<th
												scope="col"
												class="bg-muted/60 border-b px-3 py-2 text-left align-bottom text-xs font-medium"
												aria-sort={sort.key === `t:${test.test_id}` ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'}
											>
												<div class="grid min-w-28 gap-1">
													<a href="/tests/{test.test_id}/results" class="text-primary line-clamp-2 underline-offset-3 hover:underline">
														{test.title}
													</a>
													<button
														type="button"
														class="text-muted-foreground hover:text-foreground w-fit text-[11px]"
														onclick={() => (sort = nextSort(sort, `t:${test.test_id}`))}
													>
														сортировать{sort.key === `t:${test.test_id}` ? (sort.dir === 'asc' ? ' ↑' : ' ↓') : ''}
													</button>
												</div>
											</th>
										{/each}
									</tr>
								</thead>
								<tbody>
									{#each rows as row (row.user_id)}
										<tr class="group">
											<th
												scope="row"
												class="bg-background group-hover:bg-muted sticky left-0 z-10 max-w-56 border-b px-3 py-2 text-left align-top font-normal"
											>
												<div class="truncate font-medium" title={row.display_name}>{row.display_name}</div>
												<div class="text-muted-foreground truncate text-xs" title={row.email}>{row.email}</div>
											</th>
											{#each data.tests as test (test.test_id)}
												{@const cell = cellOf(row.cells, test.test_id)}
												<td class="group-hover:bg-muted/50 border-b px-3 py-2 align-top whitespace-nowrap tabular-nums">
													{#if !cell || cell.attempts === 0}
														<span class="text-muted-foreground text-xs">нет попыток</span>
													{:else}
														<span class="font-medium">{formatScore(cell.final_score)}</span>
														<span class="text-muted-foreground">/ {formatScore(cell.max_score)}</span>
														{#if formatPercent(cell.final_percent)}
															<div class="text-muted-foreground text-xs">{formatPercent(cell.final_percent)}</div>
														{/if}
														{#if cell.attempts > 1}
															<div class="text-muted-foreground text-xs">
																{count(cell.attempts, ['попытка', 'попытки', 'попыток'])}
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
					{/if}
				{/snippet}
			</LoadState>
		</div>
	{/snippet}
</RequireAuth>
