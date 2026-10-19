<script lang="ts">
	import { page } from '$app/state';
	import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import ReportFilters from '$lib/components/reports/ReportFilters.svelte';
	import ResultsTable from '$lib/components/reports/ResultsTable.svelte';
	import StatsView from '$lib/components/reports/StatsView.svelte';
	import AttemptDetailsDialog from '$lib/components/reports/AttemptDetailsDialog.svelte';
	import type { Mode } from '$lib/components/reports/table';
	import { api, unwrap } from '$lib/api/client';
	import { Resource } from '$lib/resource.svelte';
	import { count } from '$lib/format';

	const testId = $derived(page.params.id ?? '');

	let streamId = $state('');
	let mode = $state<Mode>('final');
	let tab = $state<'results' | 'stats'>('results');
	let openAttempt = $state<string | null>(null);

	const query = $derived({ stream_id: streamId || undefined, mode });

	const results = new Resource(() =>
		unwrap(api.GET('/api/tests/{test_id}/results', { params: { path: { test_id: testId }, query } }))
	);
	const stats = new Resource(() =>
		unwrap(api.GET('/api/tests/{test_id}/stats', { params: { path: { test_id: testId }, query } }))
	);
	// Потоки для фильтра — потоки курса теста (GET /api/courses/{id}/streams).
	const courseStreams = new Resource(async () => {
		const test = await unwrap(api.GET('/api/tests/{test_id}', { params: { path: { test_id: testId } } }));
		return unwrap(
			api.GET('/api/courses/{course_id}/streams', { params: { path: { course_id: test.course_id } } })
		);
	});

	$effect(() => {
		void testId;
		void courseStreams.reload();
	});

	$effect(() => {
		void testId;
		void query;
		void results.reload();
	});

	// Статистику грузим, только когда вкладка открыта (и заново при смене фильтров).
	$effect(() => {
		void testId;
		void query;
		if (tab === 'stats') void stats.reload();
	});

	const streams = $derived((courseStreams.data ?? []).map((stream) => ({ id: stream.stream_id, name: stream.name })));
</script>

<svelte:head>
	<title>Результаты{results.data ? `: ${results.data.title}` : ''} — Зачтено</title>
</svelte:head>

<RequireAuth>
	{#snippet children()}
		<div class="mx-auto grid w-full max-w-6xl grid-cols-1 gap-6 px-4 py-8 sm:px-6">
			<PageHeader
				title={results.data ? `Результаты: ${results.data.title}` : 'Результаты теста'}
				back={{ href: `/tests/${testId}`, label: 'К тесту' }}
			/>

			<ReportFilters
				{streams}
				bind:streamId
				bind:mode
				exportPath={tab === 'results' ? `/api/tests/${testId}/results` : undefined}
				exportName="results"
			/>

			<Tabs.Root bind:value={tab}>
				<Tabs.List>
					<Tabs.Trigger value="results">Результаты</Tabs.Trigger>
					<Tabs.Trigger value="stats">Статистика</Tabs.Trigger>
				</Tabs.List>

				<Tabs.Content value="results" class="mt-4 grid grid-cols-1 gap-3">
					<LoadState resource={results}>
						{#snippet children(data)}
							{#if data.rows.length === 0}
								<EmptyState icon={ClipboardListIcon} title="Пока нет сданных попыток" />
							{:else}
								<p class="text-muted-foreground text-sm">
									{mode === 'final'
										? `${count(data.rows.length, ['студент', 'студента', 'студентов'])}, итоговая попытка каждого.`
										: `${count(data.rows.length, ['попытка', 'попытки', 'попыток'])}.`}
									Нажмите на балл за вопрос, чтобы увидеть ответ.
								</p>
								<ResultsTable table={data} {mode} onopen={(id) => (openAttempt = id)} />
								<AttemptDetailsDialog bind:attemptId={openAttempt} columns={data.columns} />
							{/if}
						{/snippet}
					</LoadState>
				</Tabs.Content>

				<Tabs.Content value="stats" class="mt-4 grid grid-cols-1 gap-4">
					<LoadState resource={stats}>
						{#snippet children(data)}
							<StatsView stats={data} />
						{/snippet}
					</LoadState>
				</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</RequireAuth>
