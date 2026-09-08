<script lang="ts">
	// @feature: admin.activity_history
	import { onMount } from 'svelte';
	import { fetchAllActivity, type SourceActivityOperation } from '$lib/api/aggregator';

	const OPERATION_LABELS: Record<SourceActivityOperation['operation_type'], string> = {
		scan: 'Scan',
		file_delete: 'File deleted',
		document_merge: 'Documents merged',
		metadata_edit: 'Metadata edited',
		tag_edit: 'Tags edited',
		status_edit: 'Reading status changed',
		cover_edit: 'Cover changed',
		missing_file_maintenance: 'Missing file maintenance',
	};

	const STATUS_LABELS: Record<SourceActivityOperation['status'], string> = {
		started: 'Started',
		completed: 'Completed',
		failed: 'Failed',
		interrupted: 'Interrupted',
	};

	let operations = $state<SourceActivityOperation[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			operations = await fetchAllActivity(50);
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			loading = false;
		}
	});

	function formatTime(iso: string): string {
		const d = new Date(iso);
		return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
	}

	function actorLabel(op: SourceActivityOperation): string {
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
</script>

<svelte:head><title>Activity</title></svelte:head>

<div class="p-6">
	<div class="flex items-center gap-2 mb-6">
		<h1 class="text-2xl font-semibold tracking-tight">Activity</h1>
	</div>

	{#if loading}
		<div class="text-sm text-slate-500 dark:text-slate-400">Loading history…</div>
	{:else if error}
		<div
			class="text-sm text-red-600 dark:text-red-400 border border-red-200 dark:border-red-800
				rounded-md px-4 py-3 bg-red-50 dark:bg-red-900/20"
		>
			Could not load activity: {error}
		</div>
	{:else if operations.length === 0}
		<div class="text-sm text-slate-500 dark:text-slate-400">No activity recorded yet.</div>
	{:else}
		<ul class="space-y-2">
			{#each operations as op (op.id)}
				<li>
					<a
						href="/activity/{op.id}"
						class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between
							border border-slate-200 dark:border-slate-700 rounded-lg bg-white dark:bg-slate-800
							px-4 py-3 hover:border-accent/50 transition-colors"
					>
						<div class="flex flex-col gap-0.5">
							<span class="font-medium text-sm">
								{OPERATION_LABELS[op.operation_type]}
							</span>
							<span class="text-xs text-slate-500 dark:text-slate-400">
								{actorLabel(op)} · {formatTime(op.started_at)}
							</span>
						</div>
						<div class="flex items-center gap-3 shrink-0">
							{#if op.dry_run}
								<span
									class="text-xs px-2 py-0.5 rounded-full bg-amber-100 dark:bg-amber-900/40
										text-amber-700 dark:text-amber-300"
								>
									Dry run
								</span>
							{/if}
							<span
								class="text-xs px-2 py-0.5 rounded-full
									{op.status === 'completed'
										? 'bg-emerald-100 dark:bg-emerald-900/40 text-emerald-700 dark:text-emerald-300'
										: op.status === 'failed'
											? 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300'
											: 'bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300'}"
							>
								{STATUS_LABELS[op.status]}
							</span>
						</div>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</div>
