<script lang="ts">
	// @feature: documents.merge
	import { mergeDocuments } from '$lib/api/aggregator';
	import { refreshDocuments } from '$lib/stores/documents';
	import type { AggregatedFile, MergeOutcome } from '$lib/api/aggregator';

	interface Props {
		candidates: AggregatedFile[];
		onclose: () => void;
	}

	let { candidates, onclose }: Props = $props();

	let winnerFingerprint = $state<string | null>(null);
	let merging = $state(false);
	let plan: { winner: AggregatedFile; losers: AggregatedFile[] } | undefined;
	let error = $state<string | null>(null);
	let outcomes = $state<MergeOutcome[]>([]);
	let allSkipped = $derived(outcomes.length > 0 && outcomes.every((outcome) => outcome.status === 'skipped'));
	let failed = $derived(outcomes.filter((outcome) => outcome.status === 'failed'));

	function docLabel(doc: AggregatedFile): string {
		return doc.path.split('/').pop() ?? doc.path;
	}

	async function confirmMerge() {
		if (!winnerFingerprint || merging) return;
		if (!plan) {
			const winner = candidates.find((doc) => doc.fingerprint === winnerFingerprint);
			if (!winner) return;
			plan = { winner, losers: candidates.filter((doc) => doc.fingerprint !== winnerFingerprint) };
		}


		merging = true;
		error = null;
		try {
			const retry = outcomes.length ? failed.map((outcome) => outcome.sourceId) : undefined;
			const result = await mergeDocuments(plan.winner, plan.losers, retry);
			outcomes = [
				...outcomes.filter((outcome) => !result.some((item) => item.sourceId === outcome.sourceId)),
				...result,
			];
			await refreshDocuments();
			if (outcomes.length > 0 && outcomes.every((outcome) => outcome.status === 'merged')) onclose();
			if (outcomes.length === 0) error = 'No sources are available for this merge.';
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
	onclick={() => { if (!merging) onclose(); }}
>
	<div
		class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-96 max-w-[90vw]"
		onclick={(e) => e.stopPropagation()}
	>
		<h2 class="text-base font-semibold mb-1">Merge Documents</h2>
		<p class="text-sm text-slate-500 dark:text-slate-400 mb-4">
			{#if allSkipped}
				No source contains both the chosen document and a document to merge into it.
			{:else}
				Choose the document that will keep its metadata. All file sources from the others will be moved to it.
			{/if}
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
						onchange={() => { plan = undefined; }}
						disabled={merging || outcomes.length > 0}
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

		{#if outcomes.length}
			<ul class="text-sm mb-3" aria-live="polite">
				{#each outcomes as outcome (outcome.sourceId)}
					<li>{outcome.sourceName} · {outcome.status}{outcome.status === 'failed' ? ` — ${outcome.error}` : ''}</li>
				{/each}
			</ul>
		{/if}

		{#if error}
			<p class="text-sm text-red-500 dark:text-red-400 mb-3">{error}</p>
		{/if}

		<div class="flex gap-2 justify-end">
			<button
				onclick={onclose}
				disabled={merging}
				class="px-4 py-2 text-sm rounded-lg border border-slate-200 dark:border-slate-600
					text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
			>
				{outcomes.length ? "Close" : "Cancel"}
			</button>
			<button
				onclick={confirmMerge}
				disabled={!winnerFingerprint || merging || (outcomes.length > 0 && failed.length === 0)}
				class="px-4 py-2 text-sm rounded-lg font-medium transition-colors
					bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900
					hover:bg-slate-700 dark:hover:bg-white
					disabled:opacity-50 disabled:cursor-not-allowed"
			>
				{merging ? 'Merging…' : outcomes.length ? 'Retry failed sources' : 'Merge'}
			</button>
		</div>
	</div>
</div>
