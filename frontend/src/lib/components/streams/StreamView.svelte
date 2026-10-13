<script lang="ts">
	import PageHeader from '$lib/components/app/PageHeader.svelte';
	import LoadState from '$lib/components/app/LoadState.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import { Badge } from '$lib/components/ui/badge';
	import { api, unwrap } from '$lib/api/client';
	import { count } from '$lib/format';
	import { resource } from '$lib/resource.svelte';
	import MembersPanel from './MembersPanel.svelte';
	import CoursesPanel from './CoursesPanel.svelte';
	import InvitesPanel from './InvitesPanel.svelte';
	import InviteUsesPanel from './InviteUsesPanel.svelte';

	let { id }: { id: string } = $props();

	const stream = resource(() =>
		unwrap(api.GET('/api/streams/{stream_id}', { params: { path: { stream_id: id } } }))
	);
	const invites = resource(() =>
		unwrap(api.GET('/api/streams/{stream_id}/invites', { params: { path: { stream_id: id } } }))
	);

	let tab = $state('members');
</script>

<svelte:head>
	<title>{stream.data?.name ?? 'Поток'} — Зачтено</title>
</svelte:head>

<div class="mx-auto grid w-full max-w-5xl gap-6 px-4 py-8 sm:px-6">
	<LoadState resource={stream}>
		{#snippet children(detail)}
			{@const active = detail.members.filter((member) => !member.removed_at).length}
			<PageHeader title={detail.name} back={{ href: '/streams', label: 'Все потоки' }}>
				{#snippet badges()}
					<Badge variant="secondary">{count(active, ['участник', 'участника', 'участников'])}</Badge>
					<Badge variant="outline">{count(detail.courses.length, ['курс', 'курса', 'курсов'])}</Badge>
				{/snippet}
			</PageHeader>

			<Tabs.Root bind:value={tab} class="gap-4">
				<div class="-mx-4 overflow-x-auto px-4 sm:mx-0 sm:px-0">
					<Tabs.List>
						<Tabs.Trigger value="members">Участники</Tabs.Trigger>
						<Tabs.Trigger value="courses">Курсы</Tabs.Trigger>
						<Tabs.Trigger value="invites">Ссылки</Tabs.Trigger>
						<Tabs.Trigger value="uses">Журнал</Tabs.Trigger>
					</Tabs.List>
				</div>
				<Tabs.Content value="members">
					<MembersPanel streamId={id} members={detail.members} onchange={() => stream.reload()} />
				</Tabs.Content>
				<Tabs.Content value="courses">
					<CoursesPanel streamId={id} courses={detail.courses} onchange={() => stream.reload()} />
				</Tabs.Content>
				<Tabs.Content value="invites">
					<InvitesPanel streamId={id} {invites} />
				</Tabs.Content>
				<Tabs.Content value="uses">
					<InviteUsesPanel streamId={id} invites={invites.data ?? []} members={detail.members} />
				</Tabs.Content>
			</Tabs.Root>
		{/snippet}
	</LoadState>
</div>
