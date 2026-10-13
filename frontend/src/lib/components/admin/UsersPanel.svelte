<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import UsersIcon from '@lucide/svelte/icons/users';
	import LockIcon from '@lucide/svelte/icons/lock';
	import LockOpenIcon from '@lucide/svelte/icons/lock-open';
	import { toast } from 'svelte-sonner';
	import EmptyState from '$lib/components/app/EmptyState.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import ConfirmDialog from '$lib/components/app/ConfirmDialog.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { api, errorMessage, unwrap, type Schema } from '$lib/api/client';
	import { session } from '$lib/api/session.svelte';
	import { formatDateTime } from '$lib/format';
	import { Resource } from '$lib/resource.svelte';

	type User = Schema<'UserSummary'>;
	type Role = Schema<'GlobalRole'>;

	let search = $state('');
	let query = $state('');

	// Поиск с задержкой, чтобы не дёргать сервер на каждую букву.
	$effect(() => {
		const value = search.trim();
		const timer = setTimeout(() => (query = value), 300);
		return () => clearTimeout(timer);
	});

	// В OpenAPI `search` описан как path-параметр — шлём query напрямую.
	const users = new Resource(() =>
		unwrap(api.GET('/api/admin/users', { params: { query: { search: query.trim() || undefined } } }))
	);

	$effect(() => {
		void query;
		void users.reload();
	});

	/** Роли, которые сейчас меняются: `${userId}:${role}`. */
	let busy = $state<Record<string, boolean>>({});

	function hasRole(user: User, role: Role): boolean {
		return user.roles.includes(role);
	}

	function patchUser(id: string, change: (user: User) => User): void {
		if (!users.data) return;
		users.data = users.data.map((user) => (user.id === id ? change(user) : user));
	}

	async function toggleRole(user: User, role: Role, granted: boolean): Promise<void> {
		const key = `${user.id}:${role}`;
		busy[key] = true;
		// Оптимистично меняем, при ошибке откатываем.
		patchUser(user.id, (u) => ({
			...u,
			roles: granted ? [...new Set([...u.roles, role])] : u.roles.filter((r) => r !== role)
		}));
		try {
			await unwrap(
				api.POST('/api/admin/users/{user_id}/roles', {
					params: { path: { user_id: user.id } },
					body: { role, granted }
				})
			);
			const roleName = role === 'methodist' ? 'методиста' : 'суперпользователя';
			toast.success(`${user.display_name}: роль ${roleName} ${granted ? 'выдана' : 'снята'}`);
			if (user.id === session.user?.id) await session.load();
		} catch (error) {
			patchUser(user.id, (u) => ({
				...u,
				roles: granted ? u.roles.filter((r) => r !== role) : [...new Set([...u.roles, role])]
			}));
			toast.error(errorMessage(error));
		} finally {
			busy[key] = false;
		}
	}

	let target = $state<User | null>(null);
	let confirmOpen = $state(false);

	function askBlock(user: User): void {
		target = user;
		confirmOpen = true;
	}

	async function toggleBlock(): Promise<void> {
		if (!target) return;
		const user = target;
		const blocked = !user.blocked;
		await unwrap(
			api.POST('/api/admin/users/{user_id}/block', {
				params: { path: { user_id: user.id } },
				body: { blocked }
			})
		);
		patchUser(user.id, (u) => ({ ...u, blocked }));
		toast.success(blocked ? `${user.display_name} заблокирован(а)` : `${user.display_name} разблокирован(а)`);
	}

	const roleLabels: Array<{ role: Role; label: string }> = [
		{ role: 'methodist', label: 'Методист' },
		{ role: 'superuser', label: 'Суперпользователь' }
	];
</script>

<div class="grid gap-4">
	<div class="relative max-w-md">
		<SearchIcon class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
		<Input
			type="search"
			bind:value={search}
			placeholder="Поиск по имени или email"
			class="pl-8"
			aria-label="Поиск пользователей"
		/>
	</div>

	<LoadState resource={users}>
		{#snippet children(list)}
			{#if list.length === 0}
				<EmptyState
					icon={UsersIcon}
					title={query ? 'Никого не нашли' : 'Пользователей нет'}
					text={query ? 'Попробуйте часть имени или email.' : undefined}
				/>
			{:else}
				<ul class="bg-card divide-y rounded-xl border {users.loading ? 'opacity-70' : ''}" aria-busy={users.loading}>
					{#each list as user (user.id)}
						{@const isMe = user.id === session.user?.id}
						<li class="grid gap-3 p-4 lg:grid-cols-[1fr_auto] lg:items-center">
							<div class="min-w-0">
								<div class="flex flex-wrap items-center gap-2">
									<span class="font-medium break-words">{user.display_name}</span>
									{#if isMe}<Badge variant="secondary">это вы</Badge>{/if}
									{#if user.blocked}<Badge variant="destructive">заблокирован</Badge>{/if}
									{#if !user.email_verified}<Badge variant="outline">email не подтверждён</Badge>{/if}
									{#if user.trusted_registration}<Badge variant="outline">доверенная регистрация</Badge>{/if}
								</div>
								<div class="text-muted-foreground mt-0.5 text-sm break-all">{user.email}</div>
								<div class="text-muted-foreground mt-1 text-xs">зарегистрирован(а) {formatDateTime(user.created_at)}</div>
							</div>
							<div class="flex flex-wrap items-center gap-x-5 gap-y-3">
								{#each roleLabels as item (item.role)}
									{@const id = `role-${user.id}-${item.role}`}
									<div class="flex items-center gap-2">
										<Checkbox
											{id}
											checked={hasRole(user, item.role)}
											disabled={busy[`${user.id}:${item.role}`]}
											onCheckedChange={(checked) => toggleRole(user, item.role, checked)}
										/>
										<Label for={id} class="font-normal">{item.label}</Label>
									</div>
								{/each}
								{#if !isMe}
									<Button size="sm" variant="outline" onclick={() => askBlock(user)}>
										{#if user.blocked}<LockOpenIcon />Разблокировать{:else}<LockIcon />Заблокировать{/if}
									</Button>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{/snippet}
	</LoadState>
</div>

<ConfirmDialog
	bind:open={confirmOpen}
	title={target?.blocked ? 'Разблокировать пользователя?' : 'Заблокировать пользователя?'}
	description={target
		? target.blocked
			? `${target.display_name} (${target.email}) снова сможет входить в систему.`
			: `Все сессии ${target.display_name} (${target.email}) будут завершены, войти не получится, пока вы не разблокируете учётную запись.`
		: undefined}
	confirmLabel={target?.blocked ? 'Разблокировать' : 'Заблокировать'}
	destructive={!target?.blocked}
	onconfirm={toggleBlock}
/>
