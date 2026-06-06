<script lang="ts">
	// @feature: documents.format_picker
	import { onMount, onDestroy } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import CoverImage from '$lib/components/CoverImage.svelte';
	import DocumentDetail from '$lib/components/DocumentDetail.svelte';
	import MergeDialog from '$lib/components/MergeDialog.svelte';
	import {
		allDocuments,
		filteredDocuments,
		documentMetaMap,
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
	import type { AggregatedFile } from '$lib/api/aggregator';

	// ── Virtual list ──────────────────────────────────────────────────────────
	const ITEM_HEIGHT = 76; // estimated row height in px
	const OVERSCAN = 3;

	let selectedFingerprint = $state<string | null>(null);
	let formatPickDoc = $state<AggregatedFile | null>(null);
	let selectMode = $state(false);
	let selectedFingerprints = $state(new Set<string>());
	let mergeDialogOpen = $state(false);

	const selectedDocs = $derived(
		$filteredDocuments.filter((d) => selectedFingerprints.has(d.fingerprint)),
	);

	function toggleSelect(fingerprint: string) {
		selectedFingerprints = new Set(
			selectedFingerprints.has(fingerprint)
				? [...selectedFingerprints].filter((f) => f !== fingerprint)
				: [...selectedFingerprints, fingerprint],
		);
	}

	function exitSelectMode() {
		selectMode = false;
		selectedFingerprints = new Set();
	}
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

	// ── Reader href (primary row action) ─────────────────────────────────────
	function readerHref(doc: AggregatedFile): string {
		if (doc.type_ === 'pdf')  return `/read/pdf/${doc.fingerprint}`;
		if (doc.type_ === 'epub') return `/read/epub/${doc.fingerprint}`;
		return `/documents/${doc.fingerprint}`;
	}

	function handleRowClick(doc: AggregatedFile, e: MouseEvent) {
		if (doc.otherFormats.length > 0) {
			e.preventDefault();
			formatPickDoc = doc;
		}
	}

	// ── Details button: inline sidebar on lg+, navigate on smaller screens ───
	function handleDetailsClick(fingerprint: string, e: MouseEvent): void {
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
			<h1 class="text-xl font-semibold">Library</h1>
			<div class="flex items-center gap-1">
				{#if selectMode}
					<button
						onclick={exitSelectMode}
						class="text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-2 py-1 rounded"
					>
						Done
					</button>
				{:else}
					{#if !$isLoading}
						<button
							onclick={() => refreshDocuments()}
							class="text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-2 py-1 rounded"
						>
							Refresh
						</button>
					{/if}
					<button
						onclick={() => { selectMode = true; selectedFingerprints = new Set(); }}
						class="text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-2 py-1 rounded"
					>
						Select
					</button>
				{/if}
			</div>
		</div>

		<!-- Search -->
		<div class="relative">
			<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500" />
			<input
				type="search"
				placeholder="Search documents…"
				bind:value={$searchQuery}
				class="w-full pl-9 pr-4 py-2 rounded-lg border border-slate-200 dark:border-slate-600
					bg-slate-50 dark:bg-slate-700/50
					focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-transparent
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

		<!-- Result count -->
		{#if ($allowedTags.size > 0 || $deniedTags.size > 0 || $searchQuery.trim().length > 0) && !$isLoading && $sources.length > 0}
			<p class="mt-1.5 text-xs text-slate-400 dark:text-slate-500">
				{$filteredDocuments.length} of {$allDocuments.length} documents
			</p>
		{/if}
	</div>

	<!-- ── Body: three columns on lg+, single column below ─────────────────── -->
	<div class="flex flex-1 min-h-0">

		<!-- Document list (virtual) -->
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
							{@const docMeta = doc.document_guid ? $documentMetaMap.get(doc.document_guid) : undefined}
							<li
								class="flex items-stretch transition-colors
									{selectMode
										? selectedFingerprints.has(doc.fingerprint)
											? 'bg-slate-100 dark:bg-slate-700/60'
											: ''
										: selectedFingerprint === doc.fingerprint
											? 'bg-slate-100 dark:bg-slate-700/60'
											: ''}"
							>
								{#if selectMode}
									<button
										onclick={() => toggleSelect(doc.fingerprint)}
										class="flex items-center pl-4 pr-2"
										aria-label={selectedFingerprints.has(doc.fingerprint) ? 'Deselect' : 'Select'}
									>
										<input
											type="checkbox"
											checked={selectedFingerprints.has(doc.fingerprint)}
											readonly
											class="w-4 h-4 accent-slate-900 dark:accent-slate-100 pointer-events-none"
										/>
									</button>
								{/if}
								<!-- Primary action: open in reader (format picker for multi-format) -->
								<a
									href={readerHref(doc)}
									onclick={(e) => handleRowClick(doc, e)}
									class="flex items-start gap-3 px-4 py-3 md:px-6 flex-1 min-w-0 transition-colors
										{selectedFingerprint !== doc.fingerprint ? 'hover:bg-slate-50 dark:hover:bg-slate-800/60' : ''}"
								>
									<!-- Cover thumbnail -->
									<CoverImage
										sourceGuids={doc.sourceGuids}
										documentGuid={doc.document_guid ?? undefined}
										hasCover={doc.has_cover ?? false}
										alt=""
										class="shrink-0 w-8 h-12 rounded"
									/>
									<!-- File type badge(s) -->
									{#if doc.otherFormats.length > 0}
										<div class="mt-0.5 shrink-0 flex gap-0.5">
											{#each [doc, ...doc.otherFormats] as fmt}
												<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
													{fmt.type_ === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
													: fmt.type_ === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
													: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
													{fmt.type_}
												</span>
											{/each}
										</div>
									{:else}
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
									{/if}
									<div class="flex-1 min-w-0">
										<!-- Primary: user title or filename -->
										<p class="text-sm font-medium truncate">
											{docMeta?.title ?? basename(doc.path)}
										</p>
										<!-- Secondary: authors or full path -->
										<p class="text-xs text-slate-400 dark:text-slate-500 truncate mt-0.5">
											{docMeta?.authors?.length ? docMeta.authors.join(', ') : doc.path}
										</p>
									</div>
								</a>

								<!-- Pills: document type + tags (right-aligned, hidden on xs) -->
								<div class="hidden sm:flex items-center gap-1 px-2 shrink-0">
									{#if docMeta?.document_type}
										<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-violet-100 text-violet-700 dark:bg-violet-900/30 dark:text-violet-400">
											{docMeta.document_type}
										</span>
									{/if}
									{#each doc.tags.slice(0, 3) as tag}
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
									{#if doc.tags.length > 3}
										<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-slate-100 dark:bg-slate-700 text-slate-400 dark:text-slate-500">
											+{doc.tags.length - 3}
										</span>
									{/if}
								</div>

								<!-- Secondary actions: size + details button -->
								<div class="flex items-center gap-1.5 pr-3 md:pr-4 shrink-0">
									<span class="hidden md:block text-xs text-slate-400 dark:text-slate-500 tabular-nums">
										{formatSize(doc.size)}
									</span>
									<a
										href="/documents/{doc.fingerprint}"
										onclick={(e) => handleDetailsClick(doc.fingerprint, e)}
										aria-label="View details"
										class="p-1.5 rounded text-slate-400 dark:text-slate-500
											hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700
											{selectedFingerprint === doc.fingerprint ? 'text-slate-600 dark:text-slate-300' : ''}
											transition-colors"
									>
										<Icon name="info" class="w-4 h-4" />
									</a>
								</div>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		</div>

		<!-- Selection toolbar (shown in select mode) -->
		{#if selectMode && selectedFingerprints.size > 0}
			<div class="fixed bottom-4 left-1/2 -translate-x-1/2 z-40 flex items-center gap-3
				bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900
				px-4 py-2.5 rounded-full shadow-lg text-sm font-medium">
				<span>{selectedFingerprints.size} selected</span>
				{#if selectedFingerprints.size >= 2}
					<button
						onclick={() => (mergeDialogOpen = true)}
						class="px-3 py-1 rounded-full bg-white dark:bg-slate-900 text-slate-900 dark:text-white text-xs font-medium hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors"
					>
						Merge
					</button>
				{/if}
				<button
					onclick={() => (selectedFingerprints = new Set())}
					class="text-slate-400 dark:text-slate-500 hover:text-white dark:hover:text-slate-900 transition-colors"
					aria-label="Clear selection"
				>
					✕
				</button>
			</div>
		{/if}

		<!-- Merge dialog -->
		{#if mergeDialogOpen}
			<MergeDialog
				candidates={selectedDocs}
				onclose={() => { mergeDialogOpen = false; exitSelectMode(); }}
			/>
		{/if}

		<!-- Format picker modal -->
		{#if formatPickDoc}
			{@const allFormats = [formatPickDoc, ...formatPickDoc.otherFormats]}
			{@const pickerMeta = formatPickDoc.document_guid ? $documentMetaMap.get(formatPickDoc.document_guid) : undefined}
			<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
			<div
				class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
				onclick={() => (formatPickDoc = null)}
			>
				<div
					class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-80 max-w-[90vw]"
					onclick={(e) => e.stopPropagation()}
				>
					<h2 class="text-base font-semibold mb-1">Choose format</h2>
					<p class="text-sm text-slate-500 dark:text-slate-400 mb-4 truncate">
						{pickerMeta?.title ?? formatPickDoc.path.split('/').pop()}
					</p>
					<div class="flex flex-col gap-2">
						{#each allFormats as fmt}
							<a
								href={readerHref(fmt)}
								onclick={() => (formatPickDoc = null)}
								class="flex items-center gap-3 px-4 py-3 rounded-xl border border-slate-200 dark:border-slate-600
									hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
							>
								<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
									{fmt.type_ === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
									: fmt.type_ === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
									: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
									{fmt.type_}
								</span>
								<span class="text-sm text-slate-700 dark:text-slate-300">{fmt.path.split('/').pop()}</span>
							</a>
						{/each}
					</div>
					<button
						onclick={() => (formatPickDoc = null)}
						class="mt-4 w-full text-sm text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors"
					>
						Cancel
					</button>
				</div>
			</div>
		{/if}

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
