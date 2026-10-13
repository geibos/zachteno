<script lang="ts">
	import type { Snippet } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { toast } from 'svelte-sonner';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { errorMessage } from '$lib/api/client';

	/**
	 * Подтверждение действия. `onconfirm` может бросить ошибку — она покажется
	 * в диалоге, и он останется открытым.
	 */
	let {
		open = $bindable(false),
		title,
		description,
		confirmLabel = 'Подтвердить',
		destructive = false,
		onconfirm,
		children
	}: {
		open?: boolean;
		title: string;
		description?: string;
		confirmLabel?: string;
		destructive?: boolean;
		onconfirm: () => Promise<void> | void;
		children?: Snippet;
	} = $props();

	let pending = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (open) error = null;
	});

	async function confirm(): Promise<void> {
		pending = true;
		error = null;
		try {
			await onconfirm();
			open = false;
		} catch (err) {
			error = errorMessage(err);
			toast.error(error);
		} finally {
			pending = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{title}</Dialog.Title>
			{#if description}<Dialog.Description>{description}</Dialog.Description>{/if}
		</Dialog.Header>
		{@render children?.()}
		{#if error}<p class="text-destructive text-sm" aria-live="polite">{error}</p>{/if}
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)} disabled={pending}>Отмена</Button>
			<Button variant={destructive ? 'destructive' : 'default'} onclick={confirm} disabled={pending}>
				{#if pending}<LoaderIcon class="animate-spin" />{/if}
				{confirmLabel}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
