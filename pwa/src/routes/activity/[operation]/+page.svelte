<script lang="ts">
	// @feature: admin.activity_history
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';
	import { formatTimestamp } from '$lib/utils/datetime';
	import {
		fetchActivityDetail,
		type ActivityDetail,
		type ActivityOperation,
	} from '$lib/api/aggregator';

	const OPERATION_LABELS: Record<ActivityOperation['operation_type'], string> = {
		scan: 'Scan',
		file_delete: 'File deleted',
		document_merge: 'Documents merged',
		metadata_edit: 'Metadata edited',
		tag_edit: 'Tags edited',
		status_edit: 'Reading status changed',
		cover_edit: 'Cover changed',
		missing_file_maintenance: 'Missing file maintenance',
	};

	const STATUS_LABELS: Record<ActivityOperation['status'], string> = {
		started: 'Started',
		completed: 'Completed',
		failed: 'Failed',
		interrupted: 'Interrupted',
	};

	const OUTCOME_LABELS: Record<string, string> = {
		success: 'Success',
		completed_with_errors: 'Completed with errors',
		failed: 'Failed',
		observed: 'Observed',
		proposed: 'Proposed',
	};

	let detail = $state<ActivityDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const operationId = $derived($page.params.operation as string);

	onMount(async () => {
		try {
			detail = await fetchActivityDetail(operationId);
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			loading = false;
		}
	});

	function actorLabel(op: ActivityOperation): string {
		switch (op.actor.kind) {
			case 'user':
				return op.actor.id ?? 'User';
			case 'local_identity':
				return `${op.actor.id ?? 'local'}`;
			case 'local_session':
				return 'This device';
			case 'system':
				return 'System';
		}
	}

	function targetTitle(t: {
		target_id: string;
		title_snapshot: string | null;
		path_snapshot: string | null;
	}): string {
		return t.title_snapshot ?? t.path_snapshot ?? t.target_id;
	}
</script>

<svelte:head><title>Activity detail</title></svelte:head>

<div class="p-6">
	<a
		href="/activity"
		class="inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400 hover:text-black dark:hover:text-white mb-4"
	>
		<Icon name="arrow-left" class="w-4 h-4" />
		Back to activity
	</a>

	{#if loading}
		<div class="text-sm text-slate-500 dark:text-slate-400">Loading…</div>
	{:else if error}
		<div
			class="text-sm text-red-600 dark:text-red-400 border border-red-200 dark:border-red-800
				rounded-md px-4 py-3 bg-red-50 dark:bg-red-900/20"
		>
			Could not load activity: {error}
		</div>
	{:else if detail}
		<h1 class="text-2xl font-semibold tracking-tight mb-1">
			{OPERATION_LABELS[detail.operation_type]}
		</h1>
		<p class="text-sm text-slate-500 dark:text-slate-400 mb-4">
			{actorLabel(detail)} · {detail.channel} · {formatTimestamp(detail.started_at)}
		</p>

		<div class="flex flex-wrap items-center gap-2 mb-6">
			<span
				class="text-xs px-2 py-0.5 rounded-full
					{detail.status === 'completed'
						? 'bg-emerald-100 dark:bg-emerald-900/40 text-emerald-700 dark:text-emerald-300'
						: detail.status === 'failed'
							? 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300'
							: 'bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300'}"
			>
				{STATUS_LABELS[detail.status]}
			</span>
			{#if detail.dry_run}
				<span
					class="text-xs px-2 py-0.5 rounded-full bg-amber-100 dark:bg-amber-900/40
						text-amber-700 dark:text-amber-300"
				>
					Dry run
				</span>
			{/if}
			{#if detail.error_code}
				<span
					class="text-xs px-2 py-0.5 rounded-full bg-red-100 dark:bg-red-900/40
						text-red-700 dark:text-red-300"
				>
					{detail.error_code}
				</span>
			{/if}
		</div>

		{#if detail.events.length === 0}
			<div class="text-sm text-slate-500 dark:text-slate-400">No recorded events.</div>
		{:else}
			<ol class="relative border-l border-slate-200 dark:border-slate-700 pl-6 space-y-4">
				{#each detail.events as event (event.id)}
					<li class="relative">
						<span
							class="absolute -left-[27px] top-1 w-2.5 h-2.5 rounded-full
								{event.outcome === 'failed'
									? 'bg-red-500'
									: event.outcome === 'proposed'
										? 'bg-amber-500'
										: 'bg-emerald-500'}"
						></span>
						<div class="flex items-center gap-2">
							<span class="font-medium text-sm">{event.event_type}</span>
							<span class="text-xs text-slate-400">{formatTimestamp(event.occurred_at)}</span>
						</div>
						{#if event.targets.length > 0}
							<ul class="mt-1 space-y-0.5">
								{#each event.targets as target (target.target_id)}
									<li class="text-xs text-slate-500 dark:text-slate-400">
										{target.target_kind}: {targetTitle(target)}
									</li>
								{/each}
							</ul>
						{/if}
						<div class="text-xs text-slate-400">{OUTCOME_LABELS[event.outcome] ?? event.outcome}</div>
					</li>
				{/each}
			</ol>
		{/if}
	{/if}
</div>
