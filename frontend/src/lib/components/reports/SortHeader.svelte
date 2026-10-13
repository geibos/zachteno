<script lang="ts">
	import type { Snippet } from 'svelte';
	import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import ArrowUpDownIcon from '@lucide/svelte/icons/arrow-up-down';
	import { cn } from '$lib/utils';
	import type { SortDir, SortKey } from './table';

	let {
		sortKey,
		sort,
		onsort,
		title,
		class: className = '',
		children
	}: {
		sortKey: SortKey;
		sort: { key: SortKey; dir: SortDir };
		onsort: (key: SortKey) => void;
		title?: string;
		class?: string;
		children: Snippet;
	} = $props();

	const active = $derived(sort.key === sortKey);
</script>

<th
	scope="col"
	class={cn('bg-muted/60 border-b px-3 py-2 text-left align-bottom text-xs font-medium whitespace-nowrap', className)}
	aria-sort={active ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'}
	{title}
>
	<button
		type="button"
		class="hover:text-foreground focus-visible:ring-ring/50 inline-flex items-center gap-1 rounded outline-none focus-visible:ring-3"
		onclick={() => onsort(sortKey)}
	>
		{@render children()}
		{#if active && sort.dir === 'asc'}
			<ArrowUpIcon class="size-3.5" />
		{:else if active}
			<ArrowDownIcon class="size-3.5" />
		{:else}
			<ArrowUpDownIcon class="text-muted-foreground/60 size-3.5" />
		{/if}
	</button>
</th>
