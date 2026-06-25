<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
	import CoverImage from '$lib/components/CoverImage.svelte';
	import { allDocuments, documentMetaMap, refreshDocuments, statusFilter } from '$lib/stores/documents';
	import { sources, loadSources } from '$lib/stores/sources';
	import { fetchReadingState } from '$lib/api/aggregator';
	import type { AggregatedFile } from '$lib/api/aggregator';
	import type { DocumentMeta, ReadingStatus } from '$lib/api/client';

	interface ContinueEntry {
		doc: AggregatedFile;
		readingFormats: AggregatedFile[];
		primaryFormat: AggregatedFile;
		percentage: number;
		lastUpdated: string;
		meta: DocumentMeta | undefined;
	}

	let loading = $state(true);
	let continueReading = $state<ContinueEntry[]>([]);
	let formatPickEntry = $state<ContinueEntry | null>(null);

	// ── Stats (derived from store) ────────────────────────────────────────────
	function getDocStatus(doc: AggregatedFile): 'Reading' | 'Read' | 'Unread' {
		const all = [doc, ...doc.otherFormats];
		if (all.some((f) => f.status === 'Reading')) return 'Reading';
		if (all.some((f) => f.status === 'Read')) return 'Read';
		return 'Unread';
	}

	const totalDocs = $derived($allDocuments.length);
	const readingCount = $derived($allDocuments.filter((d) => getDocStatus(d) === 'Reading').length);
	const completedCount = $derived($allDocuments.filter((d) => getDocStatus(d) === 'Read').length);

	const formatBreakdown = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const doc of $allDocuments) {
			const types = new Set([doc, ...doc.otherFormats].map((f) => f.type_));
			for (const t of types) counts.set(t, (counts.get(t) ?? 0) + 1);
		}
		return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]);
	});

	// ── Reader href ───────────────────────────────────────────────────────────
	function readerHref(fmt: AggregatedFile): string {
		if (fmt.type_ === 'pdf') return `/read/pdf/${fmt.fingerprint}`;
		if (fmt.type_ === 'epub') return `/read/epub/${fmt.fingerprint}`;
		return `/documents/${fmt.fingerprint}`;
	}

	function handleContinueClick(entry: ContinueEntry, e: MouseEvent) {
		if (entry.readingFormats.length > 1) {
			e.preventDefault();
			formatPickEntry = entry;
		}
	}

	// ── Data loading ──────────────────────────────────────────────────────────
	onMount(async () => {
		await loadSources();
		if (get(allDocuments).length === 0) {
			await refreshDocuments();
		}
		loading = false;
		await loadContinueReading();
	});

	async function loadContinueReading() {
		const docs = get(allDocuments);
		const metaMap = get(documentMetaMap);

		const inProgress = docs.filter((doc) =>
			[doc, ...doc.otherFormats].some((f) => f.status === 'Reading'),
		);

		const entries = await Promise.all(
			inProgress.map(async (doc) => {
				const readingFormats = [doc, ...doc.otherFormats].filter((f) => f.status === 'Reading');

				const states = await Promise.all(
					readingFormats.map((f) => fetchReadingState(f.fingerprint)),
				);

				let primaryIdx = 0;
				let latestTime = '';
				states.forEach((s, i) => {
					if (s && s.last_updated > latestTime) {
						latestTime = s.last_updated;
						primaryIdx = i;
					}
				});

				return {
					doc,
					readingFormats,
					primaryFormat: readingFormats[primaryIdx],
					percentage: states[primaryIdx]?.percentage ?? 0,
					lastUpdated: latestTime,
					meta: doc.document_guid ? metaMap.get(doc.document_guid) : undefined,
				};
			}),
		);

		continueReading = entries
			.sort((a, b) => b.lastUpdated.localeCompare(a.lastUpdated))
			.slice(0, 6);
	}

	$effect(() => {
		const _ = $allDocuments;
		if (!loading) void loadContinueReading();
	});

	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function navigateToLibraryWithStatus(status: ReadingStatus) {
		statusFilter.set(status);
		goto('/library');
	}

	function pct(entry: ContinueEntry): number {
		return Math.round(entry.percentage * 100);
	}
</script>

<svelte:head>
	<title>Dashboard — Read Flow</title>
</svelte:head>

<div class="flex flex-col h-full overflow-y-auto">

	{#if loading}
		<!-- ── Loading ─────────────────────────────────────────────────────────── -->
		<div class="flex items-center justify-center gap-2 py-32 text-slate-400 dark:text-slate-500">
			<Icon name="loader" class="w-5 h-5 animate-spin" />
			<span class="text-sm">Loading…</span>
		</div>

	{:else if $sources.length === 0}
		<!-- ── Empty state: no sources ───────────────────────────────────────── -->
		<div class="px-6 py-10 md:px-10 max-w-2xl mx-auto w-full">
			<h1 class="text-2xl font-bold mb-1">Welcome to Read Flow</h1>
			<p class="text-slate-500 dark:text-slate-400 mb-8">
				Connect a remote Read Flow server to start reading your library anywhere.
			</p>

			<div class="flex flex-col gap-4">
				<!-- Step 1: Add a server -->
				<div class="flex gap-4 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
					<div class="shrink-0 w-8 h-8 rounded-full bg-accent/10 text-accent flex items-center justify-center font-bold text-sm">
						1
					</div>
					<div class="flex-1 min-w-0">
						<p class="font-medium">Add a remote server</p>
						<p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5">
							Point Read Flow at a running <code class="text-xs bg-slate-100 dark:bg-slate-700 px-1 py-0.5 rounded">read-flow-cli serve</code> instance to browse and sync your library.
						</p>
						<a
							href="/settings/sources"
							class="mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
						>
							<Icon name="server" class="w-3.5 h-3.5" />
							Add a source
						</a>
					</div>
				</div>

				<!-- Step 2: Online library -->
				<div class="flex gap-4 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
					<div class="shrink-0 w-8 h-8 rounded-full bg-slate-100 dark:bg-slate-700 text-slate-500 dark:text-slate-400 flex items-center justify-center font-bold text-sm">
						2
					</div>
					<div class="flex-1 min-w-0">
						<p class="font-medium">Discover books online</p>
						<p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5">
							Browse Project Gutenberg and other open collections to find books to add to your library.
						</p>
						<a
							href="/online-library"
							class="mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 text-sm font-medium hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
						>
							<Icon name="globe" class="w-3.5 h-3.5" />
							Browse online library
						</a>
					</div>
				</div>

				<!-- Step 3: Automatic sync -->
				<div class="flex gap-4 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
					<div class="shrink-0 w-8 h-8 rounded-full bg-slate-100 dark:bg-slate-700 text-slate-500 dark:text-slate-400 flex items-center justify-center font-bold text-sm">
						3
					</div>
					<div class="flex-1 min-w-0">
						<p class="font-medium">Read anywhere, in sync</p>
						<p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5">
							Once connected, your library and reading progress sync automatically across devices.
						</p>
					</div>
				</div>
			</div>
		</div>

	{:else}
		<!-- ── Populated dashboard ────────────────────────────────────────────── -->
		<div class="px-4 pt-6 pb-10 md:px-8 space-y-8 max-w-screen-lg mx-auto w-full">

			<!-- ── Continue Reading ──────────────────────────────────────────────── -->
			<section>
				{#if continueReading.length === 0}
					<div class="flex items-center gap-3 text-slate-400 dark:text-slate-500">
						<h2 class="text-base font-semibold text-slate-700 dark:text-slate-200">Continue Reading</h2>
						<span class="text-sm">— open a book to start tracking progress</span>
					</div>
				{:else}
					<h2 class="text-base font-semibold mb-3">Continue Reading</h2>
					<div class="flex gap-3 overflow-x-auto pb-2 -mx-4 px-4 md:-mx-8 md:px-8 snap-x snap-mandatory">
						{#each continueReading as entry (entry.primaryFormat.fingerprint)}
							<a
								href={readerHref(entry.primaryFormat)}
								onclick={(e) => handleContinueClick(entry, e)}
								class="shrink-0 w-36 snap-start flex flex-col gap-2 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 p-3 hover:border-accent/50 transition-colors"
							>
								<!-- Cover -->
								<CoverImage
									sourceGuids={entry.doc.sourceGuids}
									documentGuid={entry.doc.document_guid ?? undefined}
									hasCover={entry.doc.has_cover ?? false}
									alt=""
									class="w-full aspect-[2/3] rounded-lg"
								/>
								<!-- Title -->
								<p class="text-xs font-medium leading-snug line-clamp-2">
									{entry.meta?.title ?? basename(entry.primaryFormat.path)}
								</p>
								{#if entry.meta?.authors?.length}
									<p class="text-xs text-slate-400 dark:text-slate-500 truncate -mt-1">
										{entry.meta.authors[0]}
									</p>
								{/if}
								<!-- Progress bar -->
								<div class="space-y-1">
									<div class="h-1 bg-slate-200 dark:bg-slate-700 rounded-full overflow-hidden">
										<div
											class="h-1 bg-accent rounded-full"
											style="width: {Math.min(100, pct(entry))}%"
										></div>
									</div>
									<p class="text-xs text-slate-400 dark:text-slate-500 tabular-nums">{pct(entry)}%</p>
								</div>
							</a>
						{/each}
					</div>
				{/if}
			</section>

			<!-- ── Library Overview ──────────────────────────────────────────────── -->
			<section>
				<h2 class="text-base font-semibold mb-3">Library Overview</h2>
				<div class="grid grid-cols-3 gap-3">
					<a
						href="/library"
						class="flex flex-col gap-1 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors"
					>
						<span class="text-2xl font-bold tabular-nums">{totalDocs}</span>
						<span class="text-xs text-slate-500 dark:text-slate-400">Documents</span>
					</a>
					<button
						onclick={() => navigateToLibraryWithStatus('Reading')}
						class="flex flex-col gap-1 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors text-left"
					>
						<span class="text-2xl font-bold tabular-nums text-accent">{readingCount}</span>
						<span class="text-xs text-slate-500 dark:text-slate-400">Reading</span>
					</button>
					<button
						onclick={() => navigateToLibraryWithStatus('Read')}
						class="flex flex-col gap-1 p-4 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors text-left"
					>
						<span class="text-2xl font-bold tabular-nums text-green-500">{completedCount}</span>
						<span class="text-xs text-slate-500 dark:text-slate-400">Completed</span>
					</button>
				</div>
			</section>

			<!-- ── Format Breakdown + Quick Actions ──────────────────────────────── -->
			<div class="grid grid-cols-1 md:grid-cols-2 gap-6">

				<!-- Format Breakdown -->
				{#if formatBreakdown.length > 0}
					<section>
						<h2 class="text-base font-semibold mb-3">Formats</h2>
						<div class="bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl divide-y divide-slate-100 dark:divide-slate-700">
							{#each formatBreakdown as [type, count]}
								<div class="flex items-center justify-between px-4 py-2.5">
									<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
										{type === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
										: type === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
										: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
										{type}
									</span>
									<span class="text-sm tabular-nums text-slate-500 dark:text-slate-400">{count}</span>
								</div>
							{/each}
						</div>
					</section>
				{/if}

				<!-- Quick Actions -->
				<section>
					<h2 class="text-base font-semibold mb-3">Quick Actions</h2>
					<div class="flex flex-col gap-2">
						<a
							href="/library"
							class="flex items-center gap-3 px-4 py-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors text-sm font-medium"
						>
							<Icon name="library" class="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
							Browse Library
						</a>
						<a
							href="/online-library"
							class="flex items-center gap-3 px-4 py-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors text-sm font-medium"
						>
							<Icon name="globe" class="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
							Online Library
						</a>
						<a
							href="/settings/sources"
							class="flex items-center gap-3 px-4 py-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:border-accent/50 transition-colors text-sm font-medium"
						>
							<Icon name="server" class="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
							Manage Sources
						</a>
					</div>
				</section>
			</div>

		</div>
	{/if}

</div>

<!-- ── Format picker modal (Continue Reading multi-format) ──────────────── -->
{#if formatPickEntry}
	{@const entry = formatPickEntry}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
		onclick={() => (formatPickEntry = null)}
	>
		<div
			class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-80 max-w-[90vw]"
			onclick={(e) => e.stopPropagation()}
		>
			<h2 class="text-base font-semibold mb-1">Choose format</h2>
			<p class="text-sm text-slate-500 dark:text-slate-400 mb-4 truncate">
				{entry.meta?.title ?? basename(entry.primaryFormat.path)}
			</p>
			<div class="flex flex-col gap-2">
				{#each entry.readingFormats as fmt}
					<a
						href={readerHref(fmt)}
						onclick={() => (formatPickEntry = null)}
						class="flex items-center gap-3 px-4 py-3 rounded-xl border border-slate-200 dark:border-slate-600
							hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
					>
						<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
							{fmt.type_ === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
							: fmt.type_ === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
							: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
							{fmt.type_}
						</span>
						<span class="text-sm text-slate-700 dark:text-slate-300">{basename(fmt.path)}</span>
					</a>
				{/each}
			</div>
			<button
				onclick={() => (formatPickEntry = null)}
				class="mt-4 w-full text-sm text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors"
			>
				Cancel
			</button>
		</div>
	</div>
{/if}
