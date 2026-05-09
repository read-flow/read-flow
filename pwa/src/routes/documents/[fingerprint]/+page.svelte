<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { allDocuments, refreshDocuments } from '$lib/stores/documents';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';

	const fingerprint = $derived($page.params.fingerprint ?? '');

	let doc = $state<AggregatedFile | null>(null);
	let loading = $state(true);

	onMount(async () => {
		let docs = get(allDocuments);
		if (docs.length === 0) await refreshDocuments();
		docs = get(allDocuments);
		doc = docs.find((d) => d.fingerprint === fingerprint) ?? null;
		loading = false;
	});

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function readerHref(d: AggregatedFile): string {
		if (d.type_ === 'epub') return `/read/epub/${d.fingerprint}`;
		if (d.type_ === 'pdf') return `/read/pdf/${d.fingerprint}`;
		return '#';
	}
</script>

<svelte:head>
	<title>{doc ? basename(doc.path) : 'Document'} — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<a
		href="/"
		class="inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 mb-5 transition-colors"
	>
		<Icon name="arrow-left" class="w-4 h-4" />
		Library
	</a>

	{#if loading}
		<div class="flex items-center gap-2 text-slate-400 dark:text-slate-500">
			<Icon name="loader" class="w-5 h-5 animate-spin" />
			<span class="text-sm">Loading…</span>
		</div>
	{:else if !doc}
		<div class="flex flex-col items-center gap-3 py-16 text-center">
			<Icon name="alert-circle" class="w-8 h-8 text-slate-300 dark:text-slate-600" />
			<p class="text-sm text-slate-500 dark:text-slate-400">Document not found.</p>
		</div>
	{:else}
		<div class="space-y-5">
			<div>
				<h1 class="text-lg font-semibold text-slate-900 dark:text-slate-100 break-words">{basename(doc.path)}</h1>
				<p class="text-sm text-slate-400 dark:text-slate-500 mt-1 break-all">{doc.path}</p>
			</div>

			{#if doc.type_ === 'epub' || doc.type_ === 'pdf'}
				<a
					href={readerHref(doc)}
					class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
				>
					<Icon name="library" class="w-4 h-4" />
					Open {doc.type_.toUpperCase()}
				</a>
			{/if}

			<!-- Metadata -->
			<dl class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
				<div class="flex items-center justify-between px-4 py-3">
					<dt class="text-slate-500 dark:text-slate-400">Type</dt>
					<dd class="font-medium text-slate-900 dark:text-slate-100 uppercase">{doc.type_}</dd>
				</div>
				<div class="flex items-center justify-between px-4 py-3">
					<dt class="text-slate-500 dark:text-slate-400">Size</dt>
					<dd class="font-medium text-slate-900 dark:text-slate-100">{formatSize(doc.size)}</dd>
				</div>
				<div class="flex items-center justify-between px-4 py-3">
					<dt class="text-slate-500 dark:text-slate-400">Status</dt>
					<dd class="font-medium text-slate-900 dark:text-slate-100">{doc.status}</dd>
				</div>
				<div class="flex px-4 py-3 gap-4">
					<dt class="text-slate-500 dark:text-slate-400 shrink-0">Fingerprint</dt>
					<dd class="font-mono text-xs text-slate-500 dark:text-slate-400 break-all">{doc.fingerprint}</dd>
				</div>
			</dl>

			<!-- Tags -->
			<div>
				<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">Tags</h2>
				{#if doc.tags.length === 0}
					<p class="text-sm text-slate-400 dark:text-slate-500">No tags.</p>
				{:else}
					<div class="flex flex-wrap gap-1.5">
						{#each doc.tags as tag}
							<span class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300">
								{tag}
							</span>
						{/each}
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>
