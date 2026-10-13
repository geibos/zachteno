<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import { api, errorMessage, unwrap } from '$lib/api/client';
	import type { LinkView } from './model';

	let {
		open = $bindable(false),
		courseId,
		link,
		decision,
		ondone
	}: {
		open?: boolean;
		courseId: string;
		link: LinkView | null;
		decision: 'confirmed' | 'rejected';
		ondone: () => void;
	} = $props();

	let comment = $state('');
	let pending = $state(false);
	let error = $state<string | null>(null);

	// При открытии подставляем прежний комментарий.
	$effect(() => {
		if (open) {
			comment = link?.comment ?? '';
			error = null;
		}
	});

	async function submit(): Promise<void> {
		if (!link) return;
		pending = true;
		error = null;
		try {
			await unwrap(
				api.POST('/api/antifraud/decisions', {
					body: {
						course_id: courseId,
						user_a: link.user_a.id,
						user_b: link.user_b.id,
						decision,
						comment: comment.trim()
					}
				})
			);
			open = false;
			ondone();
		} catch (err) {
			error = errorMessage(err);
		} finally {
			pending = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{decision === 'confirmed' ? 'Подтвердить связь' : 'Отклонить связь'}</Dialog.Title>
			{#if link}
				<Dialog.Description>
					{link.user_a.display_name} ↔ {link.user_b.display_name}.
					{decision === 'confirmed'
						? 'Связь останется в группах и будет отмечена как проверенная.'
						: 'Связь не будет учитываться в группах.'}
					Решение ни на что не влияет автоматически.
				</Dialog.Description>
			{/if}
		</Dialog.Header>
		<div class="grid gap-2">
			<Label for="decision-comment">Комментарий</Label>
			<Textarea
				id="decision-comment"
				bind:value={comment}
				rows={4}
				maxlength={2000}
				placeholder="Почему вы так решили — например, «брат и сестра, живут вместе»"
			/>
		</div>
		{#if error}<p class="text-destructive text-sm" aria-live="polite">{error}</p>{/if}
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)} disabled={pending}>Отмена</Button>
			<Button variant={decision === 'rejected' ? 'destructive' : 'default'} onclick={submit} disabled={pending}>
				{#if pending}<LoaderIcon class="animate-spin" />{/if}
				{decision === 'confirmed' ? 'Подтвердить' : 'Отклонить'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
