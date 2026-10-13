<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import EyeOffIcon from '@lucide/svelte/icons/eye-off';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';

	type Props = Omit<HTMLInputAttributes, 'value' | 'type' | 'id' | 'files'> & {
		id: string;
		label: string;
		value?: string;
		type?: 'text' | 'email' | 'password';
		error?: string | null;
		hint?: string;
		/** Дополнение справа от подписи (например, ссылка «Забыли пароль?»). */
		aside?: Snippet;
	};

	let {
		id,
		label,
		value = $bindable(''),
		type = 'text',
		error = null,
		hint,
		aside,
		...rest
	}: Props = $props();

	let reveal = $state(false);
	const inputType = $derived(type === 'password' && reveal ? 'text' : type);
	const describedBy = $derived(error ? `${id}-error` : hint ? `${id}-hint` : undefined);
</script>

<div class="grid gap-2">
	<div class="flex items-center justify-between gap-2">
		<Label for={id}>{label}</Label>
		{@render aside?.()}
	</div>
	<div class="relative">
		<Input
			{id}
			type={inputType}
			bind:value
			aria-invalid={error ? true : undefined}
			aria-describedby={describedBy}
			class={type === 'password' ? 'pr-10' : undefined}
			{...rest}
		/>
		{#if type === 'password'}
			<button
				type="button"
				class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/50 absolute inset-y-0 right-0 flex w-10 items-center justify-center rounded-r-md outline-none focus-visible:ring-3"
				aria-label={reveal ? 'Скрыть пароль' : 'Показать пароль'}
				aria-pressed={reveal}
				onclick={() => (reveal = !reveal)}
			>
				{#if reveal}
					<EyeOffIcon class="size-4" />
				{:else}
					<EyeIcon class="size-4" />
				{/if}
			</button>
		{/if}
	</div>
	{#if error}
		<p id="{id}-error" class="text-destructive text-sm">{error}</p>
	{:else if hint}
		<p id="{id}-hint" class="text-muted-foreground text-sm">{hint}</p>
	{/if}
</div>
