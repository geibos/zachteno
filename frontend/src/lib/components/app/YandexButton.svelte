<script lang="ts">
	import { Button } from '$lib/components/ui/button';

	let { href, label = 'Войти через Яндекс' }: { href: string; label?: string } = $props();
</script>

<!-- Полная перезагрузка: OAuth-редирект делает сервер. -->
<Button {href} variant="outline" size="lg" class="w-full" data-sveltekit-reload>
	<svg viewBox="0 0 24 24" class="size-5" aria-hidden="true">
		<circle cx="12" cy="12" r="12" fill="#fc3f1d" />
		<text x="12" y="17" text-anchor="middle" font-size="14" font-weight="700" fill="#fff" font-family="system-ui, sans-serif">Я</text>
	</svg>
	{label}
</Button>
