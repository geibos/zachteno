<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import XIcon from '@lucide/svelte/icons/x';
	import * as Card from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { formatDateTime } from '$lib/format';
	import { cn } from '$lib/utils';
	import { decisionLabel, evidenceLabel, levelLabel, type LinkView } from './model';

	let {
		link,
		ondecide
	}: { link: LinkView; ondecide: (decision: 'confirmed' | 'rejected') => void } = $props();

	const levelClass = $derived(
		link.level === 'high'
			? 'bg-destructive/10 text-destructive dark:bg-destructive/20'
			: link.level === 'medium'
				? 'bg-amber-100 text-amber-900 dark:bg-amber-950/50 dark:text-amber-200'
				: 'bg-secondary text-secondary-foreground'
	);
</script>

<Card.Root class="gap-4">
	<Card.Header>
		<div class="flex flex-wrap items-start justify-between gap-2">
			<div class="grid min-w-0 gap-1">
				<Card.Title class="text-base leading-snug break-words">
					{link.user_a.display_name} <span class="text-muted-foreground">↔</span>
					{link.user_b.display_name}
				</Card.Title>
				<p class="text-muted-foreground text-xs break-all">{link.user_a.email} · {link.user_b.email}</p>
			</div>
			<span class={cn('inline-flex h-6 shrink-0 items-center rounded-full px-2.5 text-xs font-medium', levelClass)}>
				уровень: {levelLabel(link.level)}
			</span>
		</div>
	</Card.Header>
	<Card.Content class="grid gap-3">
		{#if link.evidence.length > 0}
			<ul class="grid gap-2">
				{#each link.evidence as item, i (i)}
					<li class="border-l-2 pl-3 text-sm">
						<span class="text-muted-foreground block text-xs">
							{evidenceLabel(item.kind)}{#if item.at} · {formatDateTime(item.at)}{/if}
						</span>
						<span class="break-words">{item.detail}</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-muted-foreground text-sm">Основания не указаны.</p>
		{/if}
		<p class="text-muted-foreground text-xs">Рассчитано {formatDateTime(link.computed_at)}</p>

		<div class="bg-muted/60 grid gap-1 rounded-lg p-3 text-sm">
			<div class="flex flex-wrap items-center gap-2">
				<span class="text-muted-foreground">Решение:</span>
				<Badge variant={link.decision === 'confirmed' ? 'default' : link.decision === 'rejected' ? 'outline' : 'secondary'}>
					{decisionLabel(link.decision)}
				</Badge>
				{#if link.decided_at}
					<span class="text-muted-foreground text-xs">{formatDateTime(link.decided_at)}</span>
				{/if}
			</div>
			{#if link.comment}
				<p class="break-words whitespace-pre-line">{link.comment}</p>
			{/if}
		</div>
	</Card.Content>
	<Card.Footer class="flex flex-wrap gap-2">
		<Button
			size="sm"
			variant={link.decision === 'confirmed' ? 'secondary' : 'default'}
			onclick={() => ondecide('confirmed')}
		>
			<CheckIcon />{link.decision === 'confirmed' ? 'Изменить комментарий' : 'Подтвердить'}
		</Button>
		<Button
			size="sm"
			variant={link.decision === 'rejected' ? 'secondary' : 'outline'}
			onclick={() => ondecide('rejected')}
		>
			<XIcon />{link.decision === 'rejected' ? 'Изменить комментарий' : 'Отклонить'}
		</Button>
	</Card.Footer>
</Card.Root>
