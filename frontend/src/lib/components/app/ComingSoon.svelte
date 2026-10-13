<script lang="ts">
	import ConstructionIcon from '@lucide/svelte/icons/construction';
	import * as Card from '$lib/components/ui/card';

	let { title, text }: { title: string; text: string } = $props();
</script>

<div class="mx-auto w-full max-w-5xl px-4 py-8 sm:px-6">
	<h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
	<Card.Root class="mt-6">
		<Card.Content class="flex flex-col items-center gap-3 py-10 text-center">
			<ConstructionIcon class="text-muted-foreground size-10" />
			<p class="font-medium">Раздел в разработке</p>
			<p class="text-muted-foreground max-w-sm text-sm">{text}</p>
		</Card.Content>
	</Card.Root>
</div>
