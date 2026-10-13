<script lang="ts">
	import CalendarClockIcon from '@lucide/svelte/icons/calendar-clock';
	import PlayIcon from '@lucide/svelte/icons/play';
	import SquareIcon from '@lucide/svelte/icons/square';
	import UsersIcon from '@lucide/svelte/icons/users';
	import InfoIcon from '@lucide/svelte/icons/info';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Alert from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import MskDateTimeInput from '$lib/components/app/MskDateTimeInput.svelte';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { count, formatDateTime } from '$lib/format';
	import { resource } from '$lib/resource.svelte';

	type WindowState = Schema<'WindowState'>;
	type WindowChange = Schema<'WindowChange'>;

	let { testId, published }: { testId: string; published: boolean } = $props();

	const windows = resource(() =>
		unwrap(api.GET('/api/tests/{test_id}/windows', { params: { path: { test_id: testId } } }))
	);

	let busy = $state<string | null>(null);

	async function change(streamId: string, body: WindowChange, success: string): Promise<void> {
		await unwrap(
			api.PUT('/api/tests/{test_id}/windows/{stream_id}', {
				params: { path: { test_id: testId, stream_id: streamId } },
				body
			})
		);
		toast.success(success);
		await windows.reload();
	}

	async function openNow(window: WindowState): Promise<void> {
		busy = window.stream_id;
		try {
			await change(window.stream_id, { action: 'open' }, `Окно открыто для «${window.stream_name}»`);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			busy = null;
		}
	}

	// Закрытие — с подтверждением.
	let closing = $state<WindowState | null>(null);
	let closeOpen = $state(false);

	// Расписание.
	let scheduling = $state<WindowState | null>(null);
	let scheduleOpen = $state(false);
	let openAt = $state<string | null>(null);
	let closeAt = $state<string | null>(null);
	let scheduleError = $state<string | null>(null);
	let scheduleSaving = $state(false);

	function startSchedule(window: WindowState): void {
		scheduling = window;
		openAt = window.open_at ?? null;
		closeAt = window.close_at ?? null;
		scheduleError = null;
		scheduleOpen = true;
	}

	async function saveSchedule(): Promise<void> {
		if (!scheduling) return;
		scheduleError = null;
		if (!openAt && !closeAt) {
			scheduleError = 'Укажите время открытия, закрытия или оба.';
			return;
		}
		if (openAt && closeAt && Date.parse(closeAt) <= Date.parse(openAt)) {
			scheduleError = 'Закрытие должно быть позже открытия.';
			return;
		}
		scheduleSaving = true;
		try {
			await change(
				scheduling.stream_id,
				{ action: 'schedule', open_at: openAt, close_at: closeAt },
				'Расписание сохранено'
			);
			scheduleOpen = false;
		} catch (error) {
			scheduleError = errorMessage(error);
		} finally {
			scheduleSaving = false;
		}
	}

	function scheduled(window: WindowState): boolean {
		return !window.is_open && !!window.open_at && Date.parse(window.open_at) > Date.now();
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title>Окна прохождения</Card.Title>
		<Card.Description>Когда студенты каждого потока могут начинать попытки. Время — по МСК.</Card.Description>
	</Card.Header>
	<Card.Content class="grid gap-4">
		<Alert.Root>
			<InfoIcon />
			<Alert.Description>
				Закрытие окна запрещает начинать новые попытки. Уже начатые попытки с лимитом времени
				дорабатывают свой срок, без лимита — сдаются при закрытии.
				{#if !published}<br /><strong>Открыть окно можно только у опубликованного теста.</strong>{/if}
			</Alert.Description>
		</Alert.Root>

		<LoadState resource={windows}>
			{#snippet children(list)}
				{#if list.length === 0}
					<EmptyState
						icon={UsersIcon}
						title="Тест ещё не назначен ни одному потоку"
						text="Назначьте курс потоку на странице потока — тогда здесь появятся окна."
					>
						<Button href="/streams" variant="outline" size="sm">Потоки</Button>
					</EmptyState>
				{:else}
					<ul class="divide-y rounded-lg border">
						{#each list as window (window.stream_id)}
							<li class="grid gap-3 p-4 sm:grid-cols-[1fr_auto] sm:items-center">
								<div class="min-w-0">
									<div class="flex flex-wrap items-center gap-2">
										<span class="font-medium break-words">{window.stream_name}</span>
										{#if window.is_open}
											<Badge class="bg-emerald-600 text-white">открыто</Badge>
										{:else if scheduled(window)}
											<Badge variant="secondary">запланировано</Badge>
										{:else}
											<Badge variant="outline">закрыто</Badge>
										{/if}
										{#if window.active_attempts > 0}
											<Badge variant="outline">
												идёт {count(window.active_attempts, ['попытка', 'попытки', 'попыток'])}
											</Badge>
										{/if}
									</div>
									<div class="text-muted-foreground mt-1 grid gap-0.5 text-xs">
										{#if window.open_at}<span>Открытие: {formatDateTime(window.open_at)}</span>{/if}
										{#if window.close_at}<span>Закрытие: {formatDateTime(window.close_at)}</span>{/if}
										{#if window.closed_at && !window.is_open}
											<span>Закрыто {formatDateTime(window.closed_at)}</span>
										{/if}
									</div>
								</div>
								<div class="flex flex-wrap gap-2">
									{#if !window.is_open}
										<Button
											size="sm"
											onclick={() => openNow(window)}
											disabled={!published || busy === window.stream_id}
										>
											{#if busy === window.stream_id}<LoaderIcon class="animate-spin" />{:else}<PlayIcon />{/if}
											Открыть сейчас
										</Button>
									{:else}
										<Button
											size="sm"
											variant="outline"
											onclick={() => {
												closing = window;
												closeOpen = true;
											}}
										>
											<SquareIcon />
											Закрыть
										</Button>
									{/if}
									<Button size="sm" variant="outline" onclick={() => startSchedule(window)}>
										<CalendarClockIcon />
										Расписание
									</Button>
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/snippet}
		</LoadState>
	</Card.Content>
</Card.Root>

<ConfirmDialog
	bind:open={closeOpen}
	title="Закрыть окно для «{closing?.stream_name ?? ''}»?"
	description={closing && closing.active_attempts > 0
		? `Сейчас идёт ${count(closing.active_attempts, ['попытка', 'попытки', 'попыток'])}. Попытки с лимитом времени доработают свой срок, без лимита — будут сданы сейчас. Новые попытки начать будет нельзя.`
		: 'Новые попытки начать будет нельзя.'}
	confirmLabel="Закрыть окно"
	destructive
	onconfirm={async () => {
		if (closing) await change(closing.stream_id, { action: 'close' }, 'Окно закрыто');
	}}
/>

<Dialog.Root bind:open={scheduleOpen}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Расписание: {scheduling?.stream_name}</Dialog.Title>
			<Dialog.Description>
				Окно откроется и закроется автоматически. Любое поле можно оставить пустым.
			</Dialog.Description>
		</Dialog.Header>
		<div class="grid gap-4">
			<MskDateTimeInput id="open-at" label="Открыть" bind:value={openAt} />
			<MskDateTimeInput id="close-at" label="Закрыть" bind:value={closeAt} />
			{#if !published}
				<p class="text-muted-foreground text-xs">Тест ещё не опубликован — опубликуйте его до открытия окна.</p>
			{/if}
			{#if scheduleError}<p class="text-destructive text-sm" aria-live="polite">{scheduleError}</p>{/if}
		</div>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (scheduleOpen = false)} disabled={scheduleSaving}>Отмена</Button>
			<Button onclick={saveSchedule} disabled={scheduleSaving}>
				{#if scheduleSaving}<LoaderIcon class="animate-spin" />{/if}
				Сохранить
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
