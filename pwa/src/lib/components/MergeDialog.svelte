<script lang="ts">
	// @feature: documents.merge
	import { mergeDocuments } from '$lib/api/aggregator';
	import { refreshDocuments } from '$lib/stores/documents';
	import type { AggregatedFile } from '$lib/api/aggregator';

	interface Props {
		candidates: AggregatedFile[];
		onclose: () => void;
	}

	let { candidates, onclose }: Props = $props();

	let winnerFingerprint = $state<string | null>(null);
	let merging = $state(false);
	let error = $state<string | null>(null);

	function docLabel(doc: AggregatedFile): string {
		return doc.path.split('/').pop() ?? doc.path;
	}

	async function confirmMerge() {
		if (!winnerFingerprint) return;
		const winner = candidates.find((d) => d.fingerprint === winnerFingerprint);
		if (!winner) return;
		const losers = candidates.filter((d) => d.fingerprint !== winnerFingerprint);

		merging = true;
		error = null;
		try {
			await mergeDocuments(winner, losers);
			await refreshDocuments();
			onclose();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Merge failed';
		} finally {
			merging = false;
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div
	class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
	onclick={onclose}
>
	<div
		class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-96 max-w-[90vw]"
		onclick={(e) => e.stopPropagation()}
	>
		<h2 class="text-base font-semibold mb-1">Merge Documents</h2>
		<p class="text-sm text-slate-500 dark:text-slate-400 mb-4">
			Choose the document that will keep its metadata. All file sources from the others will be moved to it.
		</p>

		<div class="flex flex-col gap-2 mb-4">
			{#each candidates as doc}
				<label
					class="flex items-center gap-3 px-4 py-3 rounded-xl border cursor-pointer transition-colors
						{winnerFingerprint === doc.fingerprint
							? 'border-slate-900 dark:border-slate-100 bg-slate-50 dark:bg-slate-700'
							: 'border-slate-200 dark:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700'}"
				>
					<input
						type="radio"
						name="merge-winner"
						value={doc.fingerprint}
						bind:group={winnerFingerprint}
						class="accent-slate-900 dark:accent-slate-100"
					/>
					<div class="flex-1 min-w-0">
						<p class="text-sm font-medium truncate">{docLabel(doc)}</p>
						<p class="text-xs text-slate-400 dark:text-slate-500 truncate mt-0.5">{doc.path}</p>
					</div>
					<span class="text-xs text-slate-400 dark:text-slate-500 shrink-0 uppercase">{doc.type_}</span>
				</label>
			{/each}
		</div>

		{#if error}
			<p class="text-sm text-red-500 dark:text-red-400 mb-3">{error}</p>
		{/if}

		<div class="flex gap-2 justify-end">
			<button
				onclick={onclose}
				class="px-4 py-2 text-sm rounded-lg border border-slate-200 dark:border-slate-600
					text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
			>
				Cancel
			</button>
			<button
				onclick={confirmMerge}
				disabled={!winnerFingerprint || merging}
				class="px-4 py-2 text-sm rounded-lg font-medium transition-colors
					bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900
					hover:bg-slate-700 dark:hover:bg-white
					disabled:opacity-50 disabled:cursor-not-allowed"
			>
				{merging ? 'Merging…' : 'Merge'}
			</button>
		</div>
	</div>
</div>
