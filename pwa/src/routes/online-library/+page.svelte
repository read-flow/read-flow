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

	let debounceTimer: ReturnType<typeof setTimeout> | null = null;
	let debounceCounter = 0;

	const filteredBooks = $derived(
		catalogFilter ? books.filter((b) => b.catalog_name === catalogFilter) : books,
	);

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
		if (!client) return;
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
	}

	// ── Download / import ───────────────────────────────────────────────────────
	type ImportState = 'importing' | 'done' | { failed: string };
	let importState = $state<Record<string, ImportState>>({});
	let formatPickBook = $state<OnlineBook | null>(null);

	function openFormatPicker(book: OnlineBook): void {
		if (book.formats.length === 1) {
			void startImport(book, book.formats[0]);
		} else {
			formatPickBook = book;
		}
	}

	async function startImport(book: OnlineBook, format: DownloadFormat): Promise<void> {
		formatPickBook = null;
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
</script>

<div class="max-w-3xl mx-auto px-4 py-6 md:px-6">
	<h1 class="text-xl font-semibold mb-1">Online library</h1>
	<p class="text-sm text-slate-400 dark:text-slate-500 mb-6">
		Search OPDS catalogs configured on a server and import books directly into its library.
	</p>

	{#if $sources.length === 0}
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

		<!-- Search -->
		<div class="relative">
			<Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500" />
			<input
				type="search"
				placeholder="Search online catalogs…"
				bind:value={query}
				oninput={onQueryInput}
				disabled={!client}
				class="w-full pl-9 pr-9 py-2 rounded-lg border border-slate-200 dark:border-slate-600
					bg-slate-50 dark:bg-slate-700/50
					focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-transparent
					placeholder:text-slate-400 dark:placeholder:text-slate-500 disabled:opacity-50"
			/>
			{#if query}
				<button
					onclick={clearSearch}
					aria-label="Clear search"
					class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300"
				>
					<Icon name="x" class="w-4 h-4" />
				</button>
			{/if}
		</div>

		<!-- Catalog filter -->
		{#if catalogs.length > 1}
			<div class="mt-2.5">
				<select
					bind:value={catalogFilter}
					class="rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 px-2 py-1 text-xs focus:outline-none focus:ring-2 focus:ring-accent/50"
					aria-label="Filter by catalog"
				>
					<option value={null}>All catalogs</option>
					{#each catalogs as c}
						<option value={c.name}>{c.name}</option>
					{/each}
				</select>
			</div>
		{/if}

		<!-- Results -->
		<div class="mt-6">
			{#if searching}
				<div class="flex items-center justify-center py-10 text-slate-400 dark:text-slate-500">
					<Icon name="loader" class="w-5 h-5 animate-spin mr-2" />
					Searching…
				</div>
			{:else if searchError}
				<p class="text-sm text-red-500 dark:text-red-400">{searchError}</p>
			{:else if searchedOnce && filteredBooks.length === 0}
				<p class="text-sm text-slate-400 dark:text-slate-500 text-center py-10">No results.</p>
			{:else if filteredBooks.length > 0}
				<ul class="space-y-3">
					{#each filteredBooks as book (book.id)}
						{@const state = importState[book.id]}
						<li class="flex gap-3 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 p-3">
							<div class="w-16 h-24 shrink-0 rounded-md overflow-hidden bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
								{#if book.cover_url}
									<img src={book.cover_url} alt="" class="w-full h-full object-cover" loading="lazy" />
								{:else}
									<Icon name="library" class="w-5 h-5 text-slate-400" />
								{/if}
							</div>
							<div class="flex-1 min-w-0">
								<p class="text-sm font-medium truncate">{book.title}</p>
								{#if book.authors.length > 0}
									<p class="text-xs text-slate-500 dark:text-slate-400 truncate">{book.authors.join(', ')}</p>
								{/if}
								{#if book.summary}
									<p class="mt-1 text-xs text-slate-400 dark:text-slate-500 line-clamp-2">{book.summary}</p>
								{/if}
								<div class="mt-2 flex items-center gap-2">
									<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400">
										{book.catalog_name}
									</span>
									{#if state === 'done'}
										<span class="inline-flex items-center gap-1 text-xs text-green-600 dark:text-green-400">
											<Icon name="check" class="w-3.5 h-3.5" /> Imported
										</span>
									{:else if state === 'importing'}
										<span class="inline-flex items-center gap-1 text-xs text-slate-400 dark:text-slate-500">
											<Icon name="loader" class="w-3.5 h-3.5 animate-spin" /> Importing…
										</span>
									{:else if state && typeof state === 'object'}
										<span class="text-xs text-red-500 dark:text-red-400" title={state.failed}>Import failed</span>
									{:else}
										<button
											onclick={() => openFormatPicker(book)}
											disabled={book.formats.length === 0}
											class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-xs font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
										>
											<Icon name="download" class="w-3.5 h-3.5" />
											Import
										</button>
									{/if}
								</div>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</div>

<!-- Format picker dialog -->
{#if formatPickBook}
	{@const book = formatPickBook}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
		onclick={() => (formatPickBook = null)}
	>
		<div
			class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-80 max-w-[90vw]"
			onclick={(e) => e.stopPropagation()}
		>
			<h2 class="text-base font-semibold mb-1">Choose format</h2>
			<p class="text-sm text-slate-500 dark:text-slate-400 mb-4 truncate">{book.title}</p>
			<div class="flex flex-col gap-2">
				{#each book.formats as fmt}
					<button
						onclick={() => startImport(book, fmt)}
						class="flex items-center gap-3 px-4 py-3 rounded-xl border border-slate-200 dark:border-slate-600
							hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors text-left"
					>
						<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
							bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400">
							{fmt.label}
						</span>
						<span class="text-sm text-slate-700 dark:text-slate-300 truncate">{fmt.mime_type}</span>
					</button>
				{/each}
			</div>
			<button
				onclick={() => (formatPickBook = null)}
				class="mt-4 w-full text-sm text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors"
			>
				Cancel
			</button>
		</div>
	</div>
{/if}
