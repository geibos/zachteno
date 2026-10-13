<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import UserIcon from '@lucide/svelte/icons/user';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { toast } from 'svelte-sonner';
	import Logo from './Logo.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Avatar from '$lib/components/ui/avatar';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import { session } from '$lib/api/session.svelte';
	import { errorMessage } from '$lib/api/client';
	import { initials } from '$lib/format';
	import { cn } from '$lib/utils';

	interface NavItem {
		href: string;
		label: string;
	}

	const nav = $derived.by((): NavItem[] => {
		const user = session.user;
		if (!user) return [];
		const items: NavItem[] = [{ href: '/', label: 'Панель' }];
		if (user.roles.methodist) items.push({ href: '/courses', label: 'Курсы' });
		if (user.roles.superuser) items.push({ href: '/admin', label: 'Администрирование' });
		return items;
	});

	function isActive(href: string): boolean {
		const path = page.url.pathname;
		return href === '/' ? path === '/' : path === href || path.startsWith(href + '/');
	}

	const loginHref = $derived.by(() => {
		const path = page.url.pathname;
		if (path === '/' || path === '/login' || path === '/register') return '/login';
		return `/login?next=${encodeURIComponent(path + page.url.search)}`;
	});

	async function logout(): Promise<void> {
		try {
			await session.logout();
			toast.success('Вы вышли из системы');
		} catch (error) {
			toast.error(errorMessage(error));
		}
	}
</script>

<header class="bg-background/85 sticky top-0 z-40 border-b backdrop-blur supports-[backdrop-filter]:bg-background/70">
	<div class="mx-auto flex h-14 w-full max-w-5xl items-center gap-4 px-4 sm:px-6">
		<a href="/" class="focus-visible:ring-ring/50 rounded-md outline-none focus-visible:ring-3" aria-label="Зачтено — на главную">
			<Logo />
		</a>

		{#if nav.length > 0}
			<nav class="hidden items-center gap-1 sm:flex" aria-label="Основная навигация">
				{#each nav as item (item.href)}
					<a
						href={item.href}
						class={cn(
							'rounded-md px-3 py-1.5 text-sm font-medium transition-colors',
							isActive(item.href)
								? 'bg-secondary text-secondary-foreground'
								: 'text-muted-foreground hover:text-foreground hover:bg-muted'
						)}
						aria-current={isActive(item.href) ? 'page' : undefined}
					>
						{item.label}
					</a>
				{/each}
			</nav>
		{/if}

		<div class="ml-auto flex items-center gap-2">
			{#if session.user}
				{@const user = session.user}
				<DropdownMenu.Root>
					<DropdownMenu.Trigger>
						{#snippet child({ props })}
							<Button {...props} variant="ghost" class="h-10 gap-2 px-1.5 sm:px-2" aria-label="Меню пользователя">
								<Avatar.Root class="size-7">
									<Avatar.Fallback class="bg-primary/10 text-primary text-xs font-semibold">
										{initials(user.display_name)}
									</Avatar.Fallback>
								</Avatar.Root>
								<span class="hidden max-w-40 truncate text-sm sm:inline">{user.display_name}</span>
								<ChevronDownIcon class="text-muted-foreground size-4" />
							</Button>
						{/snippet}
					</DropdownMenu.Trigger>
					<DropdownMenu.Content align="end" class="w-60">
						<DropdownMenu.Label class="font-normal">
							<div class="truncate text-sm font-medium">{user.display_name}</div>
							<div class="text-muted-foreground truncate text-xs">{user.email}</div>
						</DropdownMenu.Label>
						<DropdownMenu.Separator />
						{#each nav as item (item.href)}
							<DropdownMenu.Item class="sm:hidden" onSelect={() => goto(item.href)}>
								{item.label}
							</DropdownMenu.Item>
						{/each}
						<DropdownMenu.Item onSelect={() => goto('/profile')}>
							<UserIcon />
							Профиль
						</DropdownMenu.Item>
						<DropdownMenu.Separator />
						<DropdownMenu.Item onSelect={logout}>
							<LogOutIcon />
							Выйти
						</DropdownMenu.Item>
					</DropdownMenu.Content>
				</DropdownMenu.Root>
			{:else if session.ready}
				<Button href={loginHref} variant="ghost" size="sm">Войти</Button>
				<Button href="/register" size="sm">Регистрация</Button>
			{/if}
		</div>
	</div>
</header>
