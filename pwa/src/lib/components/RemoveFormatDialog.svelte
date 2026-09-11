<script lang="ts">
	// @feature: documents.remove_format
	import { removeContentFromDocument } from '$lib/api/aggregator';
	import { refreshDocuments } from '$lib/stores/documents';
	import { sources } from '$lib/stores/sources';
	import type { AggregatedFile } from '$lib/api/aggregator';

	interface Props {
		documentGuid: string;
		// The format being removed (fingerprint + all its copies).
		format: AggregatedFile;
		onclose: () => void;
		// Called after a successful removal (dialog will be closed by the parent).
		onremoved?: () => void;
	}

	let { documentGuid, format, onclose, onremoved }: Props = $props();

	let removing = $state(false);
	let error = $state<string | null>(null);

	/** `{sourceId, name, path}` for every copy of this format across sources. */
	const copies = $derived(
		$sources
			.filter((s) => s.id !== undefined && format.sourcePaths[s.id] !== undefined)
			.map((s) => ({
				...s,
				path: format.sourcePaths[s.id as number],
			})),
	);

	function sourceName(id: number): string {
		return $sources.find((s) => s.id === id)?.name ?? `Source ${id}`;
	}

	async function confirmRemove() {
		if (removing) return;
		removing = true;
		error = null;
		try {
			await removeContentFromDocument(documentGuid, format.fingerprint);
			await refreshDocuments();
			onremoved?.();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to remove format.';
		} finally {
			removing = false;
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div
	class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
	onclick={onclose}
>
	<div
		class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-[30rem] max-w-[92vw]"
		onclick={(e) => e.stopPropagation()}
	>
		<h2 class="text-base font-semibold mb-1 text-red-600 dark:text-red-400">
			Remove {format.type_.toUpperCase()} format?
		</h2>
		<p class="text-sm text-slate-500 dark:text-slate-400 mb-4">
			Every {format.type_.toUpperCase()} copy of this document will be permanently
			deleted, including its reading progress and tags. This cannot be undone.
		</p>

		<p class="text-xs font-medium uppercase tracking-wide text-slate-400 dark:text-slate-500 mb-2">
			Files to be deleted
		</p>
		<ul class="rounded-xl border border-red-200 dark:border-red-900/50 bg-red-50 dark:bg-red-950/30 divide-y divide-red-100 dark:divide-red-900/40 mb-4 text-sm">
			{#each copies as copy}
				<li class="px-4 py-2.5">
					<p class="text-slate-700 dark:text-slate-300 font-mono text-xs break-all">{copy.path}</p>
					<p class="text-xs text-slate-500 dark:text-slate-400 mt-0.5">{sourceName(copy.id as number)}</p>
				</li>
			{/each}
		</ul>

		{#if error}
			<p class="text-sm text-red-500 dark:text-red-400 mb-3">{error}</p>
		{/if}

		<div class="flex gap-2 justify-end">
			<button
				onclick={onclose}
				disabled={removing}
				class="px-4 py-2 text-sm rounded-lg border border-slate-200 dark:border-slate-600
					text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-50"
			>
				Cancel
			</button>
			<button
				onclick={confirmRemove}
				disabled={removing}
				class="px-4 py-2 text-sm rounded-lg font-medium transition-colors
					bg-red-600 text-white hover:bg-red-700
					disabled:opacity-50 disabled:cursor-not-allowed"
			>
				{removing ? 'Removing…' : `Remove ${copies.length} file${copies.length === 1 ? '' : 's'}`}
			</button>
		</div>
	</div>
</div>