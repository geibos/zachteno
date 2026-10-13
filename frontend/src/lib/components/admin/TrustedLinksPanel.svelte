<script lang="ts">
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { toast } from 'svelte-sonner';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import OneTimeLink from '$lib/components/app/OneTimeLink.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { count, formatDateTime } from '$lib/format';
	import { resource, Resource } from '$lib/resource.svelte';

	type Use = Schema<'TrustedLinkUse'>;

	const links = resource(() => unwrap(api.GET('/api/admin/trusted-links')));

	let label = $state('');
	let pending = $state(false);
	let issuedUrl = $state<string | null>(null);

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		pending = true;
		try {
			const issued = await unwrap(api.POST('/api/admin/trusted-links', { body: { label: label.trim() } }));
			issuedUrl = issued.url;
			label = '';
			await links.reload();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			pending = false;
		}
	}

	/** Журналы раскрытых ссылок. */
	let journals = $state<Record<string, Resource<Use[]>>>({});
	let expanded = $state<Record<string, boolean>>({});

	function toggle(linkId: string): void {
		expanded[linkId] = !expanded[linkId];
		if (expanded[linkId]) {
			const journal = new Resource(() =>
				unwrap(api.GET('/api/admin/trusted-links/{link_id}/uses', { params: { path: { link_id: linkId } } }))
			);
			journals[linkId] = journal;
			void journal.reload();
		}
	}
</script>

<div class="grid gap-6">
	<form class="bg-card grid gap-2 rounded-xl border p-4" onsubmit={create}>
		<Label for="trusted-label">Новая ссылка доверенной регистрации</Label>
		<div class="flex flex-col gap-2 sm:flex-row">
			<Input
				id="trusted-label"
				bind:value={label}
				placeholder="Метка, например «Кафедра ПИ» (необязательно)"
				class="min-w-0 sm:max-w-sm"
				maxlength={200}
			/>
			<Button type="submit" disabled={pending}>
				{#if pending}<LoaderIcon class="animate-spin" />{:else}<PlusIcon />{/if}
				Создать ссылку
			</Button>
		</div>
		<p class="text-muted-foreground text-xs">
			Кто зарегистрируется по ссылке, получит отметку «доверенная регистрация». Каждое использование
			записывается в журнал.
		</p>
	</form>

	{#if issuedUrl}
		<OneTimeLink url={issuedUrl} ondismiss={() => (issuedUrl = null)} />
	{/if}

	<LoadState resource={links}>
		{#snippet children(list)}
			{#if list.length === 0}
				<EmptyState icon={ShieldCheckIcon} title="Ссылок пока нет" />
			{:else}
				<ul class="bg-card divide-y rounded-xl border">
					{#each list as link (link.id)}
						{@const journal = journals[link.id]}
						<li class="p-4">
							<div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-4">
								<div class="min-w-0 flex-1">
									<div class="font-medium break-words">{link.label || 'Без метки'}</div>
									<div class="text-muted-foreground text-xs">
										создана {formatDateTime(link.created_at)} · {count(link.uses, ['использование', 'использования', 'использований'])}
									</div>
								</div>
								<Button
									size="sm"
									variant="outline"
									class="self-start sm:self-center"
									aria-expanded={expanded[link.id] ?? false}
									onclick={() => toggle(link.id)}
								>
									Журнал
									<ChevronDownIcon class="transition-transform {expanded[link.id] ? 'rotate-180' : ''}" />
								</Button>
							</div>
							{#if expanded[link.id] && journal}
								<div class="mt-3">
									<LoadState resource={journal}>
										{#snippet children(uses)}
											{#if uses.length === 0}
												<p class="text-muted-foreground text-sm">Ссылкой ещё не пользовались.</p>
											{:else}
												<ul class="bg-muted/40 divide-y rounded-lg border text-sm">
													{#each uses as use, index (index)}
														<li class="flex flex-col gap-0.5 px-3 py-2 sm:flex-row sm:items-center sm:gap-4">
															<span class="min-w-0 flex-1">
																<span class="font-medium">{use.display_name}</span>
																<span class="text-muted-foreground break-all"> · {use.email}</span>
															</span>
															<span class="text-muted-foreground text-xs">{formatDateTime(use.used_at)}</span>
														</li>
													{/each}
												</ul>
											{/if}
										{/snippet}
									</LoadState>
								</div>
							{/if}
						</li>
					{/each}
				</ul>
				<p class="text-muted-foreground text-xs">Полный адрес ссылки виден только при создании.</p>
			{/if}
		{/snippet}
	</LoadState>
</div>
