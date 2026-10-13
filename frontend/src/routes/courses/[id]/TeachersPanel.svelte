<script lang="ts">
	import UserPlusIcon from '@lucide/svelte/icons/user-plus';
	import UserMinusIcon from '@lucide/svelte/icons/user-minus';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { resource } from '$lib/resource.svelte';
	import { validateEmail } from '$lib/validation';

	let { courseId, canEdit }: { courseId: string; canEdit: boolean } = $props();

	const teachers = resource(() =>
		unwrap(api.GET('/api/courses/{course_id}/teachers', { params: { path: { course_id: courseId } } }))
	);

	let email = $state('');
	let adding = $state(false);
	let addError = $state<string | null>(null);

	async function add(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		addError = validateEmail(email);
		if (addError) return;
		adding = true;
		try {
			await unwrap(
				api.POST('/api/courses/{course_id}/teachers', {
					params: { path: { course_id: courseId } },
					body: { email: email.trim() }
				})
			);
			toast.success('Преподаватель назначен');
			email = '';
			await teachers.reload();
		} catch (error) {
			addError = errorMessage(error);
		} finally {
			adding = false;
		}
	}

	let removing = $state<Schema<'Teacher'> | null>(null);
	let removeOpen = $state(false);
</script>

<Card.Root>
	<Card.Header>
		<Card.Title>Преподаватели</Card.Title>
		<Card.Description>
			Видят тесты и результаты курса, управляют окнами и потоками. Редактировать тесты не могут.
		</Card.Description>
	</Card.Header>
	<Card.Content class="grid gap-4">
		<LoadState resource={teachers}>
			{#snippet children(list)}
				{#if list.length === 0}
					<p class="text-muted-foreground text-sm">Преподаватели не назначены.</p>
				{:else}
					<ul class="divide-y rounded-lg border">
						{#each list as teacher (teacher.user_id)}
							<li class="flex flex-wrap items-center gap-3 px-4 py-3">
								<div class="min-w-0 flex-1">
									<div class="font-medium break-words">{teacher.display_name}</div>
									<div class="text-muted-foreground text-xs break-all">{teacher.email}</div>
								</div>
								{#if canEdit}
									<Button
										size="sm"
										variant="ghost"
										onclick={() => {
											removing = teacher;
											removeOpen = true;
										}}
									>
										<UserMinusIcon />
										Снять
									</Button>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			{/snippet}
		</LoadState>

		{#if canEdit}
			<form class="grid gap-2" novalidate onsubmit={add}>
				<Label for="teacher-email">Назначить по email</Label>
				<div class="flex flex-col gap-2 sm:flex-row">
					<Input
						id="teacher-email"
						type="email"
						inputmode="email"
						autocomplete="off"
						placeholder="teacher@example.org"
						bind:value={email}
						aria-invalid={addError ? true : undefined}
					/>
					<Button type="submit" disabled={adding} class="shrink-0">
						{#if adding}<LoaderIcon class="animate-spin" />{:else}<UserPlusIcon />{/if}
						Назначить
					</Button>
				</div>
				<p class="text-muted-foreground text-xs">Пользователь уже должен быть зарегистрирован.</p>
				{#if addError}<p class="text-destructive text-sm" aria-live="polite">{addError}</p>{/if}
			</form>
		{/if}
	</Card.Content>
</Card.Root>

<ConfirmDialog
	bind:open={removeOpen}
	title="Снять преподавателя?"
	description={removing ? `${removing.display_name} потеряет доступ к курсу.` : undefined}
	confirmLabel="Снять"
	destructive
	onconfirm={async () => {
		if (!removing) return;
		await unwrap(
			api.DELETE('/api/courses/{course_id}/teachers/{user_id}', {
				params: { path: { course_id: courseId, user_id: removing.user_id } }
			})
		);
		toast.success('Преподаватель снят');
		await teachers.reload();
	}}
/>
