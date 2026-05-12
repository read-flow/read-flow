<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import DocumentDetail from '$lib/components/DocumentDetail.svelte';
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

	// ── Virtual list ──────────────────────────────────────────────────────────
	const ITEM_HEIGHT = 72; // estimated row height in px
	const OVERSCAN = 3;

	let selectedFingerprint = $state<string | null>(null);
	let listContainerEl: HTMLDivElement | undefined = $state();
	let containerHeight = $state(600);
	let scrollTop = $state(0);
	let resizeObserver: ResizeObserver | null = null;

	const startIndex = $derived(
		Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN),
	);
	const endIndex = $derived(
		Math.min(
			$filteredDocuments.length,
			Math.ceil((scrollTop + containerHeight) / ITEM_HEIGHT) + OVERSCAN,
		),
	);
	const visibleItems = $derived($filteredDocuments.slice(startIndex, endIndex));
	const paddingTop = $derived(startIndex * ITEM_HEIGHT);
	const paddingBottom = $derived(($filteredDocuments.length - endIndex) * ITEM_HEIGHT);

	// ── Lifecycle ─────────────────────────────────────────────────────────────
	onMount(async () => {
		await loadSources();
		if (get(allDocuments).length === 0) {
			await refreshDocuments();
		}

		if (listContainerEl) {
			containerHeight = listContainerEl.clientHeight;
			resizeObserver = new ResizeObserver(() => {
				containerHeight = listContainerEl!.clientHeight;
			});
			resizeObserver.observe(listContainerEl);
		}
	});

	onDestroy(() => {
		resizeObserver?.disconnect();
	});

	// ── Helpers ───────────────────────────────────────────────────────────────
	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	// ── Tag filter ────────────────────────────────────────────────────────────
	function cycleTag(tag: string) {
		if ($allowedTags.has(tag)) {
			allowedTags.update((s) => { const n = new Set(s); n.delete(tag); return n; });
			deniedTags.update((s) => new Set([...s, tag]));
		} else if ($deniedTags.has(tag)) {
			deniedTags.update((s) => { const n = new Set(s); n.delete(tag); return n; });
		} else {
			allowedTags.update((s) => new Set([...s, tag]));
		}
	}

	function clearTagFilters() {
		allowedTags.set(new Set());
		deniedTags.set(new Set());
	}

	// ── Row click: inline sidebar on lg+, navigate on smaller screens ─────────
	function handleDocumentClick(fingerprint: string, e: MouseEvent): void {
		if (window.innerWidth >= 1024) {
			e.preventDefault();
			selectedFingerprint = fingerprint;
		}
	}
</script>

<svelte:head>
	<title>Library — Read Flow</title>
</svelte:head>

<div class="flex flex-col h-full">

	<!-- ── Page header ──────────────────────────────────────────────────────── -->
	<div class="px-4 pt-5 pb-3 md:px-6 md:pt-6 border-b border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shrink-0">
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

		<!-- Tag filters — visible on < lg (on lg+ they live in the left sidebar) -->
		{#if $allTags.length > 0}
			<div class="lg:hidden mt-2.5 flex items-center gap-2 flex-wrap">
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

		<!-- Result count -->
		{#if ($allowedTags.size > 0 || $deniedTags.size > 0 || $searchQuery.trim().length > 0) && !$isLoading && $sources.length > 0}
			<p class="mt-1.5 text-xs text-slate-400 dark:text-slate-500">
				{$filteredDocuments.length} of {$allDocuments.length} documents
			</p>
		{/if}
	</div>

	<!-- ── Body: three columns on lg+, single column below ─────────────────── -->
	<div class="flex flex-1 min-h-0">

		<!-- Left: tag filter panel (lg+ only) -->
		{#if $allTags.length > 0}
			<aside class="hidden lg:flex flex-col w-52 shrink-0 overflow-y-auto border-r border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800">
				<div class="p-4 space-y-1.5">
					<p class="text-xs font-medium text-slate-400 dark:text-slate-500 uppercase tracking-wide pb-1">Filter by tag</p>
					{#each $allTags as tag}
						<button
							onclick={() => cycleTag(tag)}
							class="w-full text-left px-2.5 py-1.5 rounded-lg text-sm transition-colors
								{$allowedTags.has(tag)
									? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400 font-medium'
									: $deniedTags.has(tag)
										? 'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400 font-medium'
										: 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700/50'}"
						>
							{#if $allowedTags.has(tag)}<span class="mr-1">+</span>{:else if $deniedTags.has(tag)}<span class="mr-1">−</span>{/if}{tag}
						</button>
					{/each}

					{#if $allowedTags.size > 0 || $deniedTags.size > 0}
						<button
							onclick={clearTagFilters}
							class="w-full text-left px-2.5 py-1.5 rounded-lg text-xs text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors underline underline-offset-2"
						>
							Clear filters
						</button>
					{/if}
				</div>
			</aside>
		{/if}

		<!-- Center: document list (virtual) -->
		<div
			bind:this={listContainerEl}
			onscroll={(e) => (scrollTop = (e.currentTarget as HTMLElement).scrollTop)}
			class="flex-1 overflow-y-auto"
		>
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
				<!-- Virtual list spacer + visible rows -->
				<div style="padding-top: {paddingTop}px; padding-bottom: {paddingBottom}px">
					<ul class="divide-y divide-slate-100 dark:divide-slate-700/50">
						{#each visibleItems as doc (doc.fingerprint)}
							<li>
								<a
									href="/documents/{doc.fingerprint}"
									onclick={(e) => handleDocumentClick(doc.fingerprint, e)}
									class="flex items-start gap-3 px-4 py-3 md:px-6 transition-colors
										{selectedFingerprint === doc.fingerprint
											? 'bg-slate-100 dark:bg-slate-700/60'
											: 'hover:bg-slate-50 dark:hover:bg-slate-800/60'}"
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
				</div>
			{/if}
		</div>

		<!-- Right: details sidebar (lg+, shown when a document is selected) -->
		{#if selectedFingerprint}
			<aside class="hidden lg:flex flex-col w-80 shrink-0 overflow-y-auto border-l border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800">
				<DocumentDetail
					fingerprint={selectedFingerprint}
					onclose={() => (selectedFingerprint = null)}
				/>
			</aside>
		{/if}

	</div>
</div>
