<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { filteredDocuments, isLoading, loadError, refreshDocuments, searchQuery } from '$lib/stores/documents';
	import { sources, loadSources } from '$lib/stores/sources';

	onMount(async () => {
		await loadSources();
		await refreshDocuments();
	});

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}
</script>

<svelte:head>
	<title>Library — Read Flow</title>
</svelte:head>

<div class="flex flex-col h-full">
	<!-- Page header -->
	<div class="px-4 pt-5 pb-3 md:px-6 md:pt-6 border-b border-slate-200 bg-white">
		<div class="flex items-center justify-between gap-3 mb-3">
			<h1 class="text-xl font-semibold text-slate-900">Library</h1>
			{#if !$isLoading}
				<button
					onclick={() => refreshDocuments()}
					class="text-sm text-slate-500 hover:text-slate-800 transition-colors px-2 py-1 rounded"
				>
					Refresh
				</button>
			{/if}
		</div>

		<!-- Search -->
		<div class="relative">
			<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400" />
			<input
				type="search"
				placeholder="Search documents…"
				bind:value={$searchQuery}
				class="w-full pl-9 pr-4 py-2 rounded-lg border border-slate-200 bg-slate-50
					focus:outline-none focus:ring-2 focus:ring-slate-300 focus:border-transparent
					placeholder:text-slate-400 text-slate-900"
			/>
		</div>
	</div>

	<!-- Content -->
	<div class="flex-1 overflow-y-auto">
		{#if $isLoading}
			<div class="flex items-center justify-center gap-2 py-20 text-slate-400">
				<Icon name="loader" class="w-5 h-5 animate-spin" />
				<span class="text-sm">Loading documents…</span>
			</div>
		{:else if $loadError}
			<div class="flex flex-col items-center gap-3 py-20 px-6 text-center">
				<Icon name="alert-circle" class="w-8 h-8 text-red-400" />
				<p class="text-sm text-slate-600">{$loadError}</p>
			</div>
		{:else if $sources.length === 0}
			<div class="flex flex-col items-center gap-4 py-20 px-6 text-center">
				<Icon name="wifi-off" class="w-10 h-10 text-slate-300" />
				<div>
					<p class="font-medium text-slate-700">No sources configured</p>
					<p class="mt-1 text-sm text-slate-400">
						Add a remote read-flow server to start browsing your library.
					</p>
				</div>
				<a
					href="/settings/sources"
					class="mt-1 px-4 py-2 rounded-lg bg-slate-900 text-white text-sm font-medium hover:bg-slate-700 transition-colors"
				>
					Add a source
				</a>
			</div>
		{:else if $filteredDocuments.length === 0}
			<div class="flex flex-col items-center gap-3 py-20 px-6 text-center">
				<Icon name="search" class="w-8 h-8 text-slate-300" />
				<p class="text-sm text-slate-500">No documents match your search or filters.</p>
			</div>
		{:else}
			<ul class="divide-y divide-slate-100">
				{#each $filteredDocuments as doc (doc.fingerprint)}
					<li>
						<a
							href="/documents/{doc.fingerprint}"
							class="flex items-start gap-3 px-4 py-3 md:px-6 hover:bg-slate-50 transition-colors"
						>
							<!-- File type badge -->
							<span
								class="mt-0.5 shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
									{doc.type_ === 'pdf' ? 'bg-red-100 text-red-700' : doc.type_ === 'epub' ? 'bg-blue-100 text-blue-700' : 'bg-slate-100 text-slate-600'}"
							>
								{doc.type_}
							</span>

							<div class="flex-1 min-w-0">
								<p class="text-sm font-medium text-slate-900 truncate">{basename(doc.path)}</p>
								<p class="text-xs text-slate-400 truncate mt-0.5">{doc.path}</p>

								<!-- Tags (hidden on very small screens) -->
								{#if doc.tags.length > 0}
									<div class="hidden sm:flex flex-wrap gap-1 mt-1.5">
										{#each doc.tags.slice(0, 4) as tag}
											<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-slate-100 text-slate-600">
												{tag}
											</span>
										{/each}
										{#if doc.tags.length > 4}
											<span class="text-xs text-slate-400">+{doc.tags.length - 4}</span>
										{/if}
									</div>
								{/if}
							</div>

							<!-- Size (hidden on small screens) -->
							<span class="hidden md:block shrink-0 text-xs text-slate-400 mt-0.5">
								{formatSize(doc.size)}
							</span>
						</a>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>
