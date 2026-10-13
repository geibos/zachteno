<script lang="ts">
	import ImagePlusIcon from '@lucide/svelte/icons/image-plus';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { errorMessage, uploadImage, UPLOAD_TYPES } from '$lib/api/client';
	import type { ImageDraft } from './question-model';

	/** Картинка вопроса или варианта: загрузка, подпись (alt), удаление. */
	let {
		id,
		image = $bindable(null),
		disabled = false,
		compact = false
	}: { id: string; image?: ImageDraft | null; disabled?: boolean; compact?: boolean } = $props();

	let input = $state<HTMLInputElement | null>(null);
	let uploading = $state(false);
	let error = $state<string | null>(null);

	async function onchange(event: Event): Promise<void> {
		const target = event.currentTarget as HTMLInputElement;
		const file = target.files?.[0];
		target.value = '';
		if (!file) return;
		uploading = true;
		error = null;
		try {
			const uploaded = await uploadImage(file);
			image = { id: uploaded.id, url: uploaded.url, alt: image?.alt ?? '' };
		} catch (err) {
			error = errorMessage(err);
		} finally {
			uploading = false;
		}
	}
</script>

<div class="grid gap-2">
	{#if image}
		<div class="flex flex-col gap-2 sm:flex-row sm:items-start">
			<img
				src={image.url}
				alt={image.alt}
				class="bg-muted max-h-40 w-fit max-w-full rounded-md border object-contain"
				loading="lazy"
			/>
			<div class="grid min-w-0 flex-1 gap-2">
				<Input
					id="{id}-alt"
					placeholder="Описание картинки (alt) — для экранного диктора"
					aria-label="Описание картинки"
					bind:value={image.alt}
					{disabled}
				/>
				{#if !disabled}
					<div class="flex flex-wrap gap-2">
						<Button size="sm" variant="outline" onclick={() => input?.click()} disabled={uploading}>
							{#if uploading}<LoaderIcon class="animate-spin" />{:else}<ImagePlusIcon />{/if}
							Заменить
						</Button>
						<Button size="sm" variant="ghost" onclick={() => (image = null)}>
							<TrashIcon />
							Убрать
						</Button>
					</div>
				{/if}
			</div>
		</div>
	{:else if !disabled}
		<div>
			<Button
				size={compact ? 'xs' : 'sm'}
				variant="outline"
				onclick={() => input?.click()}
				disabled={uploading}
				aria-describedby="{id}-hint"
			>
				{#if uploading}<LoaderIcon class="animate-spin" />Загружаем…{:else}<ImagePlusIcon />Картинка{/if}
			</Button>
			{#if !compact}
				<span id="{id}-hint" class="text-muted-foreground ml-2 text-xs">PNG, JPEG, WebP, GIF до 10 МБ</span>
			{/if}
		</div>
	{/if}
	{#if error}<p class="text-destructive text-sm" aria-live="polite">{error}</p>{/if}
	<input
		bind:this={input}
		{id}
		type="file"
		accept={UPLOAD_TYPES.join(',')}
		class="hidden"
		{onchange}
		{disabled}
	/>
</div>
