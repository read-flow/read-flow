<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { sources, loadSources } from '$lib/stores/sources';
	import {
		ReadFlowClient,
		type OnlineBook,
		type OnlineCatalog,
		type DownloadFormat,
	} from '$lib/api/client';

	const DEBOUNCE_MS = 400;

	let selectedId = $state<number | null>(null);
	const selectedSource = $derived($sources.find((s) => s.id === selectedId) ?? null);
	const client = $derived(selectedSource ? new ReadFlowClient(selectedSource) : null);

	onMount(async () => {
		await loadSources();
		selectedId = $sources[0]?.id ?? null;
	});

	// ── Search (debounced) ──────────────────────────────────────────────────────
	let query = $state('');
	let searching = $state(false);
	let searchError = $state('');
	let books = $state<OnlineBook[]>([]);
	let catalogs = $state<OnlineCatalog[]>([]);
	let catalogFilter = $state<string | null>(null);
	let searchedOnce = $state(false);
	let layout = $state<'cards' | 'compact'>('cards');

	let debounceTimer: ReturnType<typeof setTimeout> | null = null;
	let debounceCounter = 0;

	const filteredBooks = $derived(
		catalogFilter ? books.filter((b) => b.catalog_name === catalogFilter) : books,
	);
	const hasResults = $derived(filteredBooks.length > 0);

	function onQueryInput(): void {
		if (debounceTimer) clearTimeout(debounceTimer);
		debounceCounter += 1;
		const counter = debounceCounter;
		const q = query.trim();
		if (!q) {
			searching = false;
			searchError = '';
			books = [];
			catalogs = [];
			searchedOnce = false;
			return;
		}
		debounceTimer = setTimeout(() => {
			if (counter === debounceCounter) void runSearch(q);
		}, DEBOUNCE_MS);
	}

	async function runSearch(q: string): Promise<void> {
		if (!client || !q) return;
		if (debounceTimer) clearTimeout(debounceTimer);
		searching = true;
		searchError = '';
		try {
			const result = await client.searchOnlineLibrary(q);
			books = result.books;
			catalogs = result.catalogs;
			searchedOnce = true;
		} catch (err) {
			searchError = err instanceof Error ? err.message : 'Search failed.';
		} finally {
			searching = false;
		}
	}

	function clearSearch(): void {
		if (debounceTimer) clearTimeout(debounceTimer);
		debounceCounter += 1;
		query = '';
		searching = false;
		searchError = '';
		books = [];
		catalogs = [];
		catalogFilter = null;
		searchedOnce = false;
		detailBook = null;
	}

	// ── Download / import ───────────────────────────────────────────────────────
	type ImportState = 'importing' | 'done' | { failed: string };
	let importState = $state<Record<string, ImportState>>({});
	let detailBook = $state<OnlineBook | null>(null);

	async function startImport(book: OnlineBook, format: DownloadFormat): Promise<void> {
		if (!client) return;
		importState = { ...importState, [book.id]: 'importing' };
		try {
			await client.importOnlineBook(book.title, format);
			importState = { ...importState, [book.id]: 'done' };
		} catch (err) {
			importState = {
				...importState,
				[book.id]: { failed: err instanceof Error ? err.message : 'Import failed.' },
			};
		}
	}

	function summaryPreview(s: string): string {
		return s.length > 200 ? `${s.slice(0, 200)}…` : s;
	}

	const hints = [
		{ icon: 'search' as const, title: 'Search', body: 'Find books by title, author, or keyword across all your connected catalogs' },
		{ icon: 'download' as const, title: 'Download', body: 'Get books in EPUB, PDF, and other formats with one click' },
		{ icon: 'library' as const, title: 'Grow Your Library', body: 'Downloaded books are automatically added to your local collection' },
	];
</script>

<div class="max-w-3xl mx-auto px-4 py-6 md:px-6">
	{#if $sources.length === 0}
		<h1 class="text-xl font-semibold mb-6">Online library</h1>
		<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-6 text-center">
			<p class="text-sm text-slate-500 dark:text-slate-400">No sources configured.</p>
			<a href="/settings/sources" class="mt-2 inline-block text-sm text-accent underline underline-offset-2">Add a source</a>
		</div>
	{:else}
		<!-- Source picker -->
		<label class="block mb-4">
			<span class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">Server</span>
			<select
				bind:value={selectedId}
				onchange={clearSearch}
				class="mt-1.5 w-full rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
			>
				{#each $sources as s}
					<option value={s.id}>{s.name}</option>
				{/each}
			</select>
		</label>

		{#if hasResults}
			<!-- ── Results view ──────────────────────────────────────────────────── -->
			<!-- Toolbar: search + catalog filter + layout toggle -->
			<div class="flex flex-wrap items-center gap-2 mb-4">
				<div class="relative flex-1 min-w-[12rem]">
					<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500" />
					<input
						type="search"
						placeholder="Search online catalogs…"
						bind:value={query}
						oninput={onQueryInput}
						class="w-full pl-9 pr-9 py-2 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50 placeholder:text-slate-400 dark:placeholder:text-slate-500"
					/>
					<button
						onclick={clearSearch}
						aria-label="Clear search"
						class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300"
					>
						<Icon name="x" class="w-4 h-4" />
					</button>
				</div>

				{#if catalogs.length > 1}
					<select
						bind:value={catalogFilter}
						aria-label="Filter by catalog"
						class="rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 px-2 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
					>
						<option value={null}>All catalogs</option>
						{#each catalogs as c}
							<option value={c.name}>{c.name}</option>
						{/each}
					</select>
				{/if}

				<!-- Layout toggle -->
				<div class="inline-flex rounded-lg border border-slate-200 dark:border-slate-600 overflow-hidden">
					<button
						onclick={() => (layout = 'cards')}
						class="px-2.5 py-2 text-xs font-medium transition-colors {layout === 'cards'
							? 'bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900'
							: 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700'}"
					>
						Cards
					</button>
					<button
						onclick={() => (layout = 'compact')}
						class="px-2.5 py-2 text-xs font-medium transition-colors {layout === 'compact'
							? 'bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900'
							: 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700'}"
					>
						Compact
					</button>
				</div>
			</div>

			{#if layout === 'cards'}
				<ul class="space-y-3">
					{#each filteredBooks as book (book.id)}
						{@const state = importState[book.id]}
						<li>
							<button
								onclick={() => (detailBook = book)}
								class="w-full text-left flex gap-3 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 p-3 hover:border-slate-300 dark:hover:border-slate-600 transition-colors"
							>
								<div class="w-16 h-24 shrink-0 rounded-md overflow-hidden bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
									{#if book.cover_url}
										<img src={book.cover_url} alt="" class="w-full h-full object-cover" loading="lazy" />
									{:else}
										<Icon name="library" class="w-5 h-5 text-slate-400" />
									{/if}
								</div>
								<div class="flex-1 min-w-0">
									<div class="flex items-start gap-2">
										<p class="text-sm font-medium flex-1 min-w-0 truncate">{book.title}</p>
										<span class="shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400">{book.catalog_name}</span>
									</div>
									{#if book.authors.length > 0}
										<p class="text-xs text-slate-500 dark:text-slate-400 truncate">{book.authors.join(', ')}</p>
									{/if}
									{#if book.summary}
										<p class="mt-1 text-xs text-slate-400 dark:text-slate-500 line-clamp-2">{summaryPreview(book.summary)}</p>
									{/if}
									{#if state === 'done'}
										<span class="mt-2 inline-flex items-center gap-1 text-xs text-green-600 dark:text-green-400"><Icon name="check" class="w-3.5 h-3.5" /> Added to library</span>
									{:else if state === 'importing'}
										<span class="mt-2 inline-flex items-center gap-1 text-xs text-slate-400 dark:text-slate-500"><Icon name="loader" class="w-3.5 h-3.5 animate-spin" /> Downloading…</span>
									{/if}
								</div>
							</button>
						</li>
					{/each}
				</ul>
			{:else}
				<!-- Compact -->
				<ul class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 overflow-hidden">
					{#each filteredBooks as book (book.id)}
						{@const state = importState[book.id]}
						<li>
							<button
								onclick={() => (detailBook = book)}
								class="w-full text-left flex items-center gap-3 px-4 py-2.5 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors"
							>
								<div class="flex-1 min-w-0">
									<p class="text-sm font-medium truncate">{book.title}</p>
									{#if book.authors.length > 0}
										<p class="text-xs text-slate-500 dark:text-slate-400 truncate">{book.authors.join(', ')}</p>
									{/if}
								</div>
								{#if state === 'done'}
									<Icon name="check" class="w-4 h-4 text-green-600 dark:text-green-400 shrink-0" />
								{:else if state === 'importing'}
									<Icon name="loader" class="w-4 h-4 animate-spin text-slate-400 shrink-0" />
								{/if}
								<span class="shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400">{book.catalog_name}</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		{:else}
			<!-- ── Hero / empty state ────────────────────────────────────────────── -->
			<div class="text-center pt-6 pb-4">
				<Icon name="library" class="w-16 h-16 mx-auto text-slate-300 dark:text-slate-600" />
				<h1 class="mt-4 text-2xl font-semibold">Discover Books Online</h1>
				<p class="mt-1.5 text-sm text-slate-500 dark:text-slate-400 max-w-md mx-auto">
					Search free and open catalogs worldwide, then download directly to your library
				</p>
			</div>

			<div class="max-w-md mx-auto flex items-center gap-2">
				<div class="relative flex-1">
					<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500" />
					<input
						type="search"
						placeholder="Search online catalogs…"
						bind:value={query}
						oninput={onQueryInput}
						disabled={!client}
						class="w-full pl-9 pr-3 py-2.5 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 focus:outline-none focus:ring-2 focus:ring-accent/50 placeholder:text-slate-400 dark:placeholder:text-slate-500 disabled:opacity-50"
					/>
				</div>
				<button
					onclick={() => runSearch(query.trim())}
					disabled={!client || !query.trim()}
					class="shrink-0 inline-flex items-center gap-1.5 px-4 py-2.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
				>
					Search
				</button>
			</div>

			<!-- Status -->
			{#if searching}
				<p class="mt-6 flex items-center justify-center gap-2 text-sm text-slate-400 dark:text-slate-500">
					<Icon name="loader" class="w-4 h-4 animate-spin" /> Searching…
				</p>
			{:else if searchError}
				<p class="mt-6 text-center text-sm text-red-500 dark:text-red-400">{searchError}</p>
			{:else if searchedOnce}
				<p class="mt-6 text-center text-sm text-slate-400 dark:text-slate-500">No results found</p>
			{/if}

			<!-- Hint cards -->
			<div class="mt-8 grid gap-3 sm:grid-cols-3">
				{#each hints as h}
					<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 p-4 text-center">
						<Icon name={h.icon} class="w-7 h-7 mx-auto text-slate-400 dark:text-slate-500" />
						<p class="mt-2 text-sm font-medium">{h.title}</p>
						<p class="mt-1 text-xs text-slate-400 dark:text-slate-500">{h.body}</p>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<!-- ── Book detail modal ──────────────────────────────────────────────────────── -->
{#if detailBook}
	{@const book = detailBook}
	{@const state = importState[book.id]}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/40" onclick={() => (detailBook = null)}>
		<div
			class="bg-white dark:bg-slate-800 rounded-t-2xl sm:rounded-2xl shadow-xl w-full sm:w-[32rem] max-w-[92vw] max-h-[85vh] overflow-y-auto"
			onclick={(e) => e.stopPropagation()}
		>
			<div class="p-5">
				<div class="flex justify-between items-start gap-3">
					<button onclick={() => (detailBook = null)} class="text-sm text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300">
						← Back
					</button>
					<button onclick={() => (detailBook = null)} aria-label="Close" class="text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300">
						<Icon name="x" class="w-5 h-5" />
					</button>
				</div>

				<div class="mt-3 flex gap-4">
					<div class="w-24 h-36 shrink-0 rounded-md overflow-hidden bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
						{#if book.cover_url}
							<img src={book.cover_url} alt="" class="w-full h-full object-cover" />
						{:else}
							<Icon name="library" class="w-6 h-6 text-slate-400" />
						{/if}
					</div>
					<div class="flex-1 min-w-0">
						<h2 class="text-base font-semibold">{book.title}</h2>
						{#if book.authors.length > 0}
							<p class="text-sm text-slate-500 dark:text-slate-400">{book.authors.join(', ')}</p>
						{/if}
						<span class="mt-2 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400">{book.catalog_name}</span>
					</div>
				</div>

				<p class="mt-4 text-sm text-slate-600 dark:text-slate-300 whitespace-pre-wrap">
					{book.summary ?? 'No description available'}
				</p>

				<div class="mt-5 border-t border-slate-100 dark:border-slate-700/50 pt-4">
					{#if state === 'done'}
						<p class="inline-flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400"><Icon name="check" class="w-4 h-4" /> Added to library</p>
					{:else if state === 'importing'}
						<p class="inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400"><Icon name="loader" class="w-4 h-4 animate-spin" /> Downloading…</p>
					{:else if state && typeof state === 'object'}
						<p class="text-sm text-red-500 dark:text-red-400">{state.failed}</p>
					{:else if book.formats.length === 0}
						<p class="text-sm text-slate-400 dark:text-slate-500">No downloadable formats.</p>
					{:else}
						<div class="flex flex-col gap-2">
							{#each book.formats as fmt}
								<button
									onclick={() => startImport(book, fmt)}
									class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
								>
									<Icon name="download" class="w-4 h-4" />
									Download {fmt.label}
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		</div>
	</div>
{/if}
