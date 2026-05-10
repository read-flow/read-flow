<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import {
		allDocuments,
		filteredDocuments,
		isLoading,
		loadError,
		refreshDocuments,
		searchQuery,
		allowedTags,
		deniedTags,
		allTags,
	} from '$lib/stores/documents';
	import { sources, loadSources } from '$lib/stores/sources';
	import { get } from 'svelte/store';

	onMount(async () => {
		await loadSources();
		// Only fetch from the server on the initial load (empty store).
		// Subsequent visits reuse the in-memory store; the Refresh button
		// triggers an explicit reload.
		if (get(allDocuments).length === 0) {
			await refreshDocuments();
		}
	});

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function cycleTag(tag: string) {
		if ($allowedTags.has(tag)) {
			// allowed → denied
			allowedTags.update((s) => { const n = new Set(s); n.delete(tag); return n; });
			deniedTags.update((s) => new Set([...s, tag]));
		} else if ($deniedTags.has(tag)) {
			// denied → off
			deniedTags.update((s) => { const n = new Set(s); n.delete(tag); return n; });
		} else {
			// off → allowed
			allowedTags.update((s) => new Set([...s, tag]));
		}
	}

	function clearTagFilters() {
		allowedTags.set(new Set());
		deniedTags.set(new Set());
	}
</script>

<svelte:head>
	<title>Library — Read Flow</title>
</svelte:head>

<div class="flex flex-col h-full">
	<!-- Page header -->
	<div class="px-4 pt-5 pb-3 md:px-6 md:pt-6 border-b border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800">
		<div class="flex items-center justify-between gap-3 mb-3">
			<h1 class="text-xl font-semibold text-slate-900 dark:text-slate-100">Library</h1>
			{#if !$isLoading}
				<button
					onclick={() => refreshDocuments()}
					class="text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-2 py-1 rounded"
				>
					Refresh
				</button>
			{/if}
		</div>

		<!-- Search -->
		<div class="relative">
			<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500" />
			<input
				type="search"
				placeholder="Search documents…"
				bind:value={$searchQuery}
				class="w-full pl-9 pr-4 py-2 rounded-lg border border-slate-200 dark:border-slate-600
					bg-slate-50 dark:bg-slate-700/50 text-slate-900 dark:text-slate-100
					focus:outline-none focus:ring-2 focus:ring-slate-300 dark:focus:ring-slate-600 focus:border-transparent
					placeholder:text-slate-400 dark:placeholder:text-slate-500"
			/>
		</div>

		<!-- Tag filters -->
		{#if $allTags.length > 0}
			<div class="mt-2.5 flex items-center gap-2 flex-wrap">
				{#each $allTags as tag}
					<button
						onclick={() => cycleTag(tag)}
						class="px-2 py-0.5 rounded-full text-xs font-medium transition-colors
							{$allowedTags.has(tag)
								? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400 ring-1 ring-green-400 dark:ring-green-600'
								: $deniedTags.has(tag)
									? 'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400 ring-1 ring-red-400 dark:ring-red-600'
									: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-600'}"
					>
						{#if $allowedTags.has(tag)}+{:else if $deniedTags.has(tag)}−{/if}{tag}
					</button>
				{/each}

				{#if $allowedTags.size > 0 || $deniedTags.size > 0}
					<button
						onclick={clearTagFilters}
						class="px-2 py-0.5 rounded-full text-xs text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors underline underline-offset-2"
					>
						Clear filters
					</button>
				{/if}
			</div>
		{/if}

		<!-- Result count (shown when filtering or searching) -->
		{#if ($allowedTags.size > 0 || $deniedTags.size > 0 || $searchQuery.trim().length > 0) && !$isLoading && $sources.length > 0}
			<p class="mt-1.5 text-xs text-slate-400 dark:text-slate-500">
				{$filteredDocuments.length} of {$allDocuments.length} documents
			</p>
		{/if}
	</div>

	<!-- Content -->
	<div class="flex-1 overflow-y-auto">
		{#if $isLoading}
			<div class="flex items-center justify-center gap-2 py-20 text-slate-400 dark:text-slate-500">
				<Icon name="loader" class="w-5 h-5 animate-spin" />
				<span class="text-sm">Loading documents…</span>
			</div>
		{:else if $loadError}
			<div class="flex flex-col items-center gap-3 py-20 px-6 text-center">
				<Icon name="alert-circle" class="w-8 h-8 text-red-400" />
				<p class="text-sm text-slate-600 dark:text-slate-400">{$loadError}</p>
			</div>
		{:else if $sources.length === 0}
			<div class="flex flex-col items-center gap-4 py-20 px-6 text-center">
				<Icon name="wifi-off" class="w-10 h-10 text-slate-300 dark:text-slate-600" />
				<div>
					<p class="font-medium text-slate-700 dark:text-slate-300">No sources configured</p>
					<p class="mt-1 text-sm text-slate-400 dark:text-slate-500">
						Add a remote read-flow server to start browsing your library.
					</p>
				</div>
				<a
					href="/settings/sources"
					class="mt-1 px-4 py-2 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
				>
					Add a source
				</a>
			</div>
		{:else if $filteredDocuments.length === 0}
			<div class="flex flex-col items-center gap-3 py-20 px-6 text-center">
				<Icon name="search" class="w-8 h-8 text-slate-300 dark:text-slate-600" />
				<p class="text-sm text-slate-500 dark:text-slate-400">No documents match your search or filters.</p>
				{#if $allowedTags.size > 0 || $deniedTags.size > 0 || $searchQuery.trim().length > 0}
					<button
						onclick={() => { $searchQuery = ''; clearTagFilters(); }}
						class="text-sm text-slate-500 dark:text-slate-400 underline underline-offset-2 hover:text-slate-700 dark:hover:text-slate-300"
					>
						Clear all filters
					</button>
				{/if}
			</div>
		{:else}
			<ul class="divide-y divide-slate-100 dark:divide-slate-700/50">
				{#each $filteredDocuments as doc (doc.fingerprint)}
					<li>
						<a
							href="/documents/{doc.fingerprint}"
							class="flex items-start gap-3 px-4 py-3 md:px-6 hover:bg-slate-50 dark:hover:bg-slate-800/60 transition-colors"
						>
							<!-- File type badge -->
							<span
								class="mt-0.5 shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
									{doc.type_ === 'pdf'
										? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
										: doc.type_ === 'epub'
											? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
											: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}"
							>
								{doc.type_}
							</span>

							<div class="flex-1 min-w-0">
								<p class="text-sm font-medium text-slate-900 dark:text-slate-100 truncate">{basename(doc.path)}</p>
								<p class="text-xs text-slate-400 dark:text-slate-500 truncate mt-0.5">{doc.path}</p>

								{#if doc.tags.length > 0}
									<div class="hidden sm:flex flex-wrap gap-1 mt-1.5">
										{#each doc.tags.slice(0, 4) as tag}
											<span
												class="inline-flex items-center px-1.5 py-0.5 rounded text-xs
													{$allowedTags.has(tag)
														? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
														: $deniedTags.has(tag)
															? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
															: 'bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-400'}"
											>
												{tag}
											</span>
										{/each}
										{#if doc.tags.length > 4}
											<span class="text-xs text-slate-400 dark:text-slate-500">+{doc.tags.length - 4}</span>
										{/if}
									</div>
								{/if}
							</div>

							<span class="hidden md:block shrink-0 text-xs text-slate-400 dark:text-slate-500 mt-0.5">
								{formatSize(doc.size)}
							</span>
						</a>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>
