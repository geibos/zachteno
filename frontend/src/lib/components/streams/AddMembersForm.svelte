<script lang="ts">
	import UserPlusIcon from '@lucide/svelte/icons/user-plus';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { count } from '$lib/format';
	import { parseEmailList } from './email-list';

	type Outcome = Schema<'AddOutcome'>;
	type Status = Schema<'AddStatus'>;

	let { streamId, onchange }: { streamId: string; onchange: () => void } = $props();

	let text = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);
	let report = $state<Outcome[] | null>(null);

	const emails = $derived(parseEmailList(text));

	const STATUS: Record<Status, { label: string; tone: 'ok' | 'muted' | 'bad' }> = {
		added: { label: 'добавлен(а)', tone: 'ok' },
		restored: { label: 'возвращён(а) в поток', tone: 'ok' },
		already_member: { label: 'уже в потоке', tone: 'muted' },
		not_found: { label: 'не зарегистрирован(а)', tone: 'bad' },
		invalid: { label: 'некорректный адрес', tone: 'bad' }
	};

	async function submit(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (emails.length === 0) return;
		saving = true;
		error = null;
		try {
			const result = await unwrap(
				api.POST('/api/streams/{stream_id}/members', {
					params: { path: { stream_id: streamId } },
					body: { emails }
				})
			);
			report = result.results;
			const joined = result.results.filter((item) => item.status === 'added' || item.status === 'restored');
			if (joined.length > 0) {
				toast.success(`В поток добавлено: ${count(joined.length, ['студент', 'студента', 'студентов'])}`);
				onchange();
			}
			// В поле остаются только адреса, которые не удалось добавить, — их можно поправить.
			text = result.results
				.filter((item) => STATUS[item.status].tone === 'bad')
				.map((item) => item.email)
				.join('\n');
		} catch (failure) {
			error = errorMessage(failure);
		} finally {
			saving = false;
		}
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title class="flex items-center gap-2 text-base"><UserPlusIcon class="size-4" />Добавить студентов</Card.Title>
		<Card.Description>
			Только тех, кто уже зарегистрирован. Вставьте адреса столбцом или через запятую — например, из таблицы
			деканата. Незарегистрированным отправьте пригласительную ссылку.
		</Card.Description>
	</Card.Header>
	<Card.Content>
		<form class="grid gap-3" onsubmit={submit}>
			<Label for="add-members" class="sr-only">Email студентов</Label>
			<Textarea
				id="add-members"
				bind:value={text}
				rows={4}
				placeholder={'ivanov@example.org\npetrova@example.org'}
				autocomplete="off"
				spellcheck={false}
			/>
			<div class="flex flex-wrap items-center gap-3">
				<Button type="submit" disabled={saving || emails.length === 0}>
					{#if saving}<LoaderIcon class="animate-spin" />{:else}<UserPlusIcon />{/if}
					Добавить
				</Button>
				{#if emails.length > 0}
					<span class="text-muted-foreground text-sm">{count(emails.length, ['адрес', 'адреса', 'адресов'])}</span>
				{/if}
			</div>
			{#if error}<p class="text-destructive text-sm" aria-live="polite">{error}</p>{/if}
		</form>

		{#if report}
			<ul class="mt-4 divide-y rounded-lg border text-sm" aria-label="Итоги добавления">
				{#each report as outcome (outcome.email)}
					<li class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
						<span class="min-w-0 break-all">
							{#if outcome.display_name}<span class="font-medium">{outcome.display_name}</span> ·{/if}
							{outcome.email}
						</span>
						<span class="ml-auto flex flex-wrap gap-1.5">
							{#if outcome.email_verified === false}
								<Badge variant="destructive">email не подтверждён</Badge>
							{/if}
							<Badge
								variant={STATUS[outcome.status].tone === 'ok'
									? 'default'
									: STATUS[outcome.status].tone === 'bad'
										? 'outline'
										: 'secondary'}
							>
								{STATUS[outcome.status].label}
							</Badge>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</Card.Content>
</Card.Root>
