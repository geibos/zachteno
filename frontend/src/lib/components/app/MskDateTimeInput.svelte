<script lang="ts">
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { isoToMskInput, mskInputToIso } from '$lib/format';

	/** Ввод даты и времени по МСК; наружу — ISO в UTC или `null`. */
	let {
		id,
		label,
		value = $bindable(null),
		disabled = false,
		hint
	}: { id: string; label: string; value?: string | null; disabled?: boolean; hint?: string } = $props();

	let local = $state(isoToMskInput(value));
	// Последнее значение, отданное наружу: отличаем внешнее изменение от своего.
	let emitted: string | null | undefined = value;

	$effect(() => {
		if (value !== emitted) {
			emitted = value;
			local = isoToMskInput(value);
		}
	});

	function oninput(): void {
		emitted = local === '' ? null : mskInputToIso(local);
		value = emitted;
	}
</script>

<div class="grid gap-2">
	<Label for={id}>{label}</Label>
	<div class="flex items-center gap-2">
		<Input {id} type="datetime-local" bind:value={local} {oninput} {disabled} class="min-w-0 flex-1" />
		<span class="text-muted-foreground shrink-0 text-sm">МСК</span>
	</div>
	{#if hint}<p class="text-muted-foreground text-xs">{hint}</p>{/if}
</div>
