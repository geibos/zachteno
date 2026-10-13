<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import type { Schema } from '$lib/api/client';

	let { status }: { status: Schema<'TestStatus'> } = $props();

	const labels: Record<Schema<'TestStatus'>, string> = {
		draft: 'черновик',
		published: 'опубликован',
		archived: 'в архиве'
	};
</script>

<Badge variant={status === 'published' ? 'default' : status === 'draft' ? 'secondary' : 'outline'}>
	{labels[status]}
</Badge>
