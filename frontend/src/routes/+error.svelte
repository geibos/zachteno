<script lang="ts">
	import { page } from '$app/state';
	import { Button } from '$lib/components/ui/button';

	const notFound = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{notFound ? 'Страница не найдена' : 'Ошибка'} — Зачтено</title>
</svelte:head>

<div class="mx-auto flex w-full max-w-md flex-col items-center gap-3 px-4 py-20 text-center">
	<p class="text-primary text-5xl font-semibold">{page.status}</p>
	<h1 class="text-xl font-semibold">{notFound ? 'Страница не найдена' : 'Что-то пошло не так'}</h1>
	<p class="text-muted-foreground text-sm">
		{notFound ? 'Возможно, ссылка устарела или в адресе опечатка.' : 'Попробуйте обновить страницу.'}
	</p>
	<Button href="/" class="mt-3">На главную</Button>
</div>
