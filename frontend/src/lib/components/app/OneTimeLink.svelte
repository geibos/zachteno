<script lang="ts">
	import CopyIcon from '@lucide/svelte/icons/copy';
	import CheckIcon from '@lucide/svelte/icons/check';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import { toast } from 'svelte-sonner';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	/** Ссылка с секретным токеном: сервер показывает её один раз. */
	let { url, ondismiss }: { url: string; ondismiss?: () => void } = $props();

	let copied = $state(false);
	let input = $state<HTMLInputElement | null>(null);

	async function copy(): Promise<void> {
		try {
			await navigator.clipboard.writeText(url);
			copied = true;
			toast.success('Ссылка скопирована');
		} catch {
			input?.select();
			toast.info('Выделили ссылку — скопируйте её вручную (Ctrl+C).');
		}
	}
</script>

<Alert.Root class="border-primary/30 bg-primary/5">
	<KeyRoundIcon class="text-primary" />
	<Alert.Title>Скопируйте ссылку сейчас</Alert.Title>
	<Alert.Description>
		<p>Полный адрес показывается только один раз — потом его не будет видно даже вам.</p>
		<div class="mt-2 flex w-full flex-col gap-2 sm:flex-row">
			<Input bind:ref={input} readonly value={url} class="font-mono text-xs" onfocus={() => input?.select()} />
			<Button onclick={copy} class="shrink-0">
				{#if copied}<CheckIcon />Скопировано{:else}<CopyIcon />Копировать{/if}
			</Button>
		</div>
		{#if ondismiss}
			<Button variant="ghost" size="sm" class="mt-1 -ml-2" onclick={ondismiss}>Готово, скрыть</Button>
		{/if}
	</Alert.Description>
</Alert.Root>
