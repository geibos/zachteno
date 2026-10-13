<script lang="ts">
	import { page } from '$app/state';
	import RequireAuth from '$lib/components/app/RequireAuth.svelte';
	import StreamView from '$lib/components/streams/StreamView.svelte';

	const id = $derived(page.params.id ?? '');
</script>

<RequireAuth>
	{#snippet children()}
		{#key id}
			<StreamView {id} />
		{/key}
	{/snippet}
</RequireAuth>
