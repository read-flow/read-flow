<script lang="ts">
	// @feature: documents.detail_view
	// @feature: documents.edit_metadata
	import Icon from '$lib/components/Icon.svelte';
	import CoverImage from '$lib/components/CoverImage.svelte';
	import {
		allDocuments,
		documentMetaMap,
		refreshDocuments,
		allTags,
		findByFingerprint,
	} from '$lib/stores/documents';
	import {
		addTagsToFile,
		removeTagsFromFile,
		updateDocumentMetadata,
		updateReadingStatus,
		deleteFileFromSources,
		sendFileToSource,
	} from '$lib/api/aggregator';
	import { sources } from '$lib/stores/sources';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';
	import type { DocumentMeta, DocumentType, ReadingStatus } from '$lib/api/client';

	const EMPTY_META: DocumentMeta = {
		document_type: null, title: null, subtitle: null, authors: null, description: null,
		language: null, publisher: null, identifier: null, date: null, subject: null,
		selected_cover_fingerprint: null,
	};

	const DOC_TYPES: DocumentType[] = [
		'Book', 'Article', 'ResearchPaper', 'Thesis', 'Letter', 'Magazine', 'Manual', 'Report',
	];

	interface Props {
		fingerprint: string;
		onclose?: () => void;
	}

	let { fingerprint, onclose }: Props = $props();

	let doc = $state<AggregatedFile | null>(null);
	let loading = $state(true);
	let newTag = $state('');
	let saving = $state(false);
	let tagError = $state('');
	let statusUpdating = $state(false);

	// ── User metadata editing ─────────────────────────────────────────────────
	let editingMeta = $state(false);
	let metaDraft = $state<DocumentMeta>({ ...EMPTY_META });
	let authorsList = $state<string[]>([]);
	let metaSaving = $state(false);
	let metaError = $state('');

	function startEditMeta() {
		const current = doc?.document_guid ? $documentMetaMap.get(doc.document_guid) : undefined;
		metaDraft = {
			document_type: current?.document_type ?? null,
			title: current?.title ?? null,
			subtitle: current?.subtitle ?? null,
			authors: current?.authors ?? null,
			description: current?.description ?? null,
			language: current?.language ?? null,
			publisher: current?.publisher ?? null,
			identifier: current?.identifier ?? null,
			date: current?.date ?? null,
			subject: current?.subject ?? null,
			selected_cover_fingerprint: current?.selected_cover_fingerprint ?? null,
		};
		authorsList = [...(metaDraft.authors ?? [])];
		metaError = '';
		editingMeta = true;
	}

	function cancelEditMeta() {
		editingMeta = false;
		metaError = '';
	}

	async function saveMeta() {
		if (!doc || metaSaving) return;
		metaSaving = true;
		metaError = '';
		const authors = authorsList.map((a) => a.trim()).filter(Boolean);
		const payload: DocumentMeta = {
			...metaDraft,
			authors: authors.length ? authors : null,
		};
		try {
			// Pass sourceGuids so the aggregator can create the document record when needed.
			await updateDocumentMetadata(doc.document_guid, payload, doc.sourceGuids);
			await refreshDocuments();
			editingMeta = false;
		} catch (err) {
			metaError = err instanceof Error ? err.message : 'Failed to save metadata.';
		} finally {
			metaSaving = false;
		}
	}

	$effect(() => {
		// Re-runs whenever `fingerprint` changes, so switching rows updates the pane.
		const fp = fingerprint;
		loading = true;
		doc = null;
		newTag = '';
		tagError = '';
		void (async () => {
			let docs = get(allDocuments);
			if (docs.length === 0) await refreshDocuments();
			docs = get(allDocuments);
			// Guard against a later click overtaking this async lookup
			if (fp === fingerprint) {
				doc = findByFingerprint(docs, fp);
				loading = false;
			}
		})();
	});

	async function changeStatus(newStatus: ReadingStatus): Promise<void> {
		if (!doc || statusUpdating) return;
		statusUpdating = true;
		try {
			await updateReadingStatus(doc.sourceGuids, doc.fingerprint, newStatus);
			await refreshDocuments();
		} catch (err) {
			console.error('Failed to update reading status:', err);
		} finally {
			statusUpdating = false;
		}
	}

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

	async function removeTag(tag: string) {
		if (!doc || saving) return;
		doc = { ...doc, tags: doc.tags.filter((t) => t !== tag) };
		saving = true;
		tagError = '';
		try {
			await removeTagsFromFile(doc.sourceGuids, [tag]);
			await refreshDocuments();
		} catch {
			tagError = `Failed to remove tag "${tag}".`;
			const docs = get(allDocuments);
			doc = findByFingerprint(docs, fingerprint) ?? doc;
		} finally {
			saving = false;
		}
	}

	async function addTag() {
		const tag = newTag.trim();
		if (!doc || !tag || saving) return;
		if (doc.tags.includes(tag)) {
			newTag = '';
			return;
		}
		doc = { ...doc, tags: [...doc.tags, tag] };
		newTag = '';
		saving = true;
		tagError = '';
		try {
			await addTagsToFile(doc.sourceGuids, [tag]);
			await refreshDocuments();
			const docs = get(allDocuments);
			doc = findByFingerprint(docs, fingerprint) ?? doc;
		} catch {
			tagError = `Failed to add tag "${tag}".`;
			const docs = get(allDocuments);
			doc = findByFingerprint(docs, fingerprint) ?? doc;
		} finally {
			saving = false;
		}
	}

	// ── Formats: cover selection, delete, send-to-source ───────────────────────
	let manageFormats = $state(false);
	let pendingDeleteFp = $state<string | null>(null);
	let formatBusy = $state(false);
	let formatError = $state('');

	/** All formats (primary + others) as a flat array. */
	const formats = $derived(doc ? [doc, ...doc.otherFormats] : []);
	/** Currently selected cover fingerprint (explicit, else the primary format). */
	const selectedCoverFp = $derived(
		(doc?.document_guid ? $documentMetaMap.get(doc.document_guid)?.selected_cover_fingerprint : null) ??
			doc?.fingerprint ??
			null,
	);

	// @feature: documents.select_cover
	async function selectCover(fp: string): Promise<void> {
		if (!doc || formatBusy) return;
		formatBusy = true;
		formatError = '';
		const current = doc.document_guid ? $documentMetaMap.get(doc.document_guid) : undefined;
		const payload: DocumentMeta = { ...EMPTY_META, ...current, selected_cover_fingerprint: fp };
		try {
			await updateDocumentMetadata(doc.document_guid, payload, doc.sourceGuids);
			await refreshDocuments();
		} catch (err) {
			formatError = err instanceof Error ? err.message : 'Failed to set cover.';
		} finally {
			formatBusy = false;
		}
	}

	async function deleteFormat(fmt: AggregatedFile): Promise<void> {
		if (formatBusy) return;
		formatBusy = true;
		formatError = '';
		const wasViewedFormat = fmt.fingerprint === fingerprint;
		try {
			await deleteFileFromSources(fmt.sourceGuids);
			pendingDeleteFp = null;
			await refreshDocuments();
			// If the format we were viewing is gone, close the pane (or clear doc).
			if (wasViewedFormat) {
				onclose?.();
			} else {
				doc = findByFingerprint(get(allDocuments), fingerprint) ?? doc;
			}
		} catch (err) {
			formatError = err instanceof Error ? err.message : 'Failed to delete format.';
		} finally {
			formatBusy = false;
		}
	}

	/** Configured sources that do NOT already hold this format. */
	function missingSources(fmt: AggregatedFile) {
		return $sources.filter((s) => s.id !== undefined && fmt.sourceGuids[s.id] === undefined);
	}

	async function sendFormat(fmt: AggregatedFile, targetSourceId: number): Promise<void> {
		if (formatBusy) return;
		formatBusy = true;
		formatError = '';
		try {
			await sendFileToSource(fmt.sourceGuids, basename(fmt.path), targetSourceId);
			await refreshDocuments();
			doc = findByFingerprint(get(allDocuments), fingerprint) ?? doc;
		} catch (err) {
			formatError = err instanceof Error ? err.message : 'Failed to send file.';
		} finally {
			formatBusy = false;
		}
	}
</script>

<div class="px-4 py-5 md:px-5 space-y-5">
	{#if onclose}
		<!-- Sidebar close button (title lives in the hero below) -->
		<div class="flex justify-end">
			<button
				onclick={onclose}
				class="shrink-0 p-1.5 rounded-lg text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
				aria-label="Close details"
			>
				<Icon name="x" class="w-4 h-4" />
			</button>
		</div>
	{/if}

	{#if loading}
		<div class="flex items-center gap-2 text-slate-400 dark:text-slate-500">
			<Icon name="loader" class="w-4 h-4 animate-spin" />
			<span class="text-sm">Loading…</span>
		</div>
	{:else if !doc}
		<div class="flex flex-col items-center gap-3 py-10 text-center">
			<Icon name="alert-circle" class="w-7 h-7 text-slate-300 dark:text-slate-600" />
			<p class="text-sm text-slate-500 dark:text-slate-400">Document not found.</p>
		</div>
	{:else}
		{@const docMeta = doc.document_guid ? $documentMetaMap.get(doc.document_guid) : undefined}

		<!-- Hero: cover alongside title / subtitle / authors / description.
		     Stacks vertically when narrow, side-by-side when wide. -->
		{#if !editingMeta}
			<div class="flex flex-col sm:flex-row gap-5">
				{#if doc.has_cover}
					<CoverImage
						sourceGuids={doc.sourceGuids}
						documentGuid={doc.document_guid ?? undefined}
						hasCover={true}
						alt={docMeta?.title ?? basename(doc.path)}
						class="w-full max-w-[160px] sm:w-40 sm:shrink-0 h-[240px] rounded-lg shadow mx-auto sm:mx-0"
					/>
				{/if}
				<div class="min-w-0 flex-1 space-y-2">
					<h1 class="text-xl font-semibold break-words leading-tight">
						{docMeta?.title ?? basename(doc.path)}
					</h1>
					{#if docMeta?.subtitle}
						<p class="text-base text-slate-500 dark:text-slate-400 break-words">{docMeta.subtitle}</p>
					{/if}
					{#if docMeta?.authors?.length}
						<p class="text-sm font-medium text-slate-600 dark:text-slate-300">{docMeta.authors.join(', ')}</p>
					{/if}
					{#if docMeta?.description}
						<hr class="border-slate-200 dark:border-slate-700" />
						<p class="text-sm text-slate-600 dark:text-slate-400 break-words whitespace-pre-line">{docMeta.description}</p>
					{/if}
				</div>
			</div>
		{/if}

		<!-- Cover selection (only when more than one format has a cover) -->
		{#if !editingMeta}
			{@const coverFormats = formats.filter((f) => f.has_cover)}
			{#if coverFormats.length >= 2}
				<div>
					<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">Cover</h2>
					<div class="flex flex-wrap gap-3">
						{#each coverFormats as fmt}
							<button
								onclick={() => selectCover(fmt.fingerprint)}
								disabled={formatBusy}
								class="flex flex-col items-center gap-1 rounded-lg p-1 transition-colors disabled:opacity-50
									{selectedCoverFp === fmt.fingerprint
										? 'ring-2 ring-accent bg-slate-50 dark:bg-slate-700/50'
										: 'hover:bg-slate-50 dark:hover:bg-slate-700/50'}"
								aria-label="Use {fmt.type_} cover"
							>
								<CoverImage
									sourceGuids={fmt.sourceGuids}
									hasCover={true}
									alt=""
									class="w-16 h-24 rounded"
								/>
								<span class="text-xs uppercase text-slate-500 dark:text-slate-400">{fmt.type_}</span>
							</button>
						{/each}
					</div>
				</div>
			{/if}
		{/if}

		<!-- Reading status -->
		<div class="flex items-center justify-between">
			<span class="text-sm text-slate-500 dark:text-slate-400">Reading status</span>
			<select
				value={doc.status}
				disabled={statusUpdating}
				onchange={(e) => void changeStatus((e.currentTarget as HTMLSelectElement).value as 'Unread' | 'Reading' | 'Read')}
				class="text-sm font-medium rounded px-2 py-0.5 border border-slate-200 dark:border-slate-600
					bg-white dark:bg-slate-800 cursor-pointer disabled:opacity-50
					{doc.status === 'Read' ? 'text-green-600 dark:text-green-400'
					: doc.status === 'Reading' ? 'text-blue-600 dark:text-blue-400'
					: 'text-slate-500 dark:text-slate-400'}"
			>
				<option value="Unread">Unread</option>
				<option value="Reading">Reading</option>
				<option value="Read">Read</option>
			</select>
		</div>

		<!-- Formats -->
		<div>
			<div class="flex items-center justify-between mb-2">
				<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300">Formats</h2>
				<button
					onclick={() => { manageFormats = !manageFormats; pendingDeleteFp = null; formatError = ''; }}
					class="text-xs text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 transition-colors"
				>
					{manageFormats ? 'Done' : 'Manage'}
				</button>
			</div>
			<ul class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
				{#each formats as fmt}
					<li class="px-4 py-3">
						<div class="flex items-center gap-3">
							<!-- Per-file cover thumbnail (file's own cover, not document-selected) -->
							<CoverImage
								sourceGuids={fmt.sourceGuids}
								hasCover={fmt.has_cover ?? false}
								alt=""
								class="shrink-0 w-8 h-12 rounded"
							/>
							<div class="flex-1 min-w-0">
								<p class="text-slate-700 dark:text-slate-300 truncate" title={fmt.path}>
									{basename(fmt.path)}
								</p>
								<div class="flex items-center gap-2 mt-0.5">
									<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
										{fmt.type_ === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
										: fmt.type_ === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
										: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
										{fmt.type_}
									</span>
									<span class="text-xs text-slate-400 dark:text-slate-500 tabular-nums">
										{formatSize(fmt.size)}
									</span>
								</div>
							</div>
							{#if manageFormats}
								<button
									onclick={() => { pendingDeleteFp = fmt.fingerprint; formatError = ''; }}
									disabled={formatBusy}
									aria-label="Delete this format"
									class="shrink-0 p-1.5 rounded-lg text-slate-400 dark:text-slate-500 hover:text-red-500 dark:hover:text-red-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
								>
									<Icon name="trash" class="w-4 h-4" />
								</button>
							{:else if fmt.type_ === 'epub' || fmt.type_ === 'pdf'}
								<a
									href={readerHref(fmt)}
									class="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-xs font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
								>
									<Icon name="library" class="w-3.5 h-3.5" />
									Open {fmt.type_.toUpperCase()}
								</a>
							{/if}
						</div>

						<!-- Delete confirmation -->
						{#if pendingDeleteFp === fmt.fingerprint}
							<div class="mt-2 flex items-center justify-between gap-2 pl-11">
								<span class="text-xs text-red-500 dark:text-red-400">Delete this format from all sources?</span>
								<div class="flex gap-2 shrink-0">
									<button
										onclick={() => deleteFormat(fmt)}
										disabled={formatBusy}
										class="text-xs px-2.5 py-1 rounded-lg bg-red-600 text-white font-medium hover:bg-red-700 transition-colors disabled:opacity-40"
									>
										{formatBusy ? 'Deleting…' : 'Delete'}
									</button>
									<button
										onclick={() => (pendingDeleteFp = null)}
										disabled={formatBusy}
										class="text-xs px-2.5 py-1 rounded-lg border border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
									>
										Cancel
									</button>
								</div>
							</div>
						{/if}

						<!-- Send to other sources -->
						{#if manageFormats}
							{@const missing = missingSources(fmt)}
							{#if missing.length > 0}
								<div class="mt-2 flex flex-wrap items-center gap-2 pl-11">
									<span class="text-xs text-slate-400 dark:text-slate-500">Send to:</span>
									{#each missing as s}
										<button
											onclick={() => sendFormat(fmt, s.id as number)}
											disabled={formatBusy}
											class="text-xs px-2.5 py-1 rounded-lg border border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
										>
											{s.name}
										</button>
									{/each}
								</div>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
			{#if formatError}
				<p class="mt-2 text-xs text-red-500 dark:text-red-400">{formatError}</p>
			{/if}
		</div>

		<!-- User-editable metadata -->
		<div>
			<div class="flex items-center justify-between mb-2">
					<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300">Document Info</h2>
					{#if editingMeta}
						<div class="flex gap-2">
							<button
								onclick={saveMeta}
								disabled={metaSaving}
								class="text-xs px-2.5 py-1 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
							>
								{metaSaving ? 'Saving…' : 'Save'}
							</button>
							<button
								onclick={cancelEditMeta}
								disabled={metaSaving}
								class="text-xs px-2.5 py-1 rounded-lg border border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
							>
								Cancel
							</button>
						</div>
					{:else}
						<button
							onclick={startEditMeta}
							class="text-xs text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 transition-colors"
							aria-label="Edit document info"
						>
							<Icon name="edit" class="w-3.5 h-3.5" />
						</button>
					{/if}
				</div>

				{#if editingMeta}
					<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
						<!-- Document type -->
						<div class="flex items-center justify-between px-4 py-3">
							<label for="meta-type-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Type</label>
							<select
								id="meta-type-{fingerprint}"
								bind:value={metaDraft.document_type}
								class="ml-3 flex-1 min-w-0 bg-transparent text-right focus:outline-none"
							>
								<option value={null}>Not set</option>
								{#each DOC_TYPES as t}
									<option value={t}>{t}</option>
								{/each}
							</select>
						</div>
						<!-- Title -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-title-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Title</label>
							<input
								id="meta-title-{fingerprint}"
								type="text"
								bind:value={metaDraft.title}
								placeholder={basename(doc.path)}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none"
							/>
						</div>
						<!-- Subtitle -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-subtitle-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Subtitle</label>
							<input
								id="meta-subtitle-{fingerprint}"
								type="text"
								bind:value={metaDraft.subtitle}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none"
							/>
						</div>
						<!-- Authors -->
						<div class="px-4 py-3">
							<p class="text-slate-500 dark:text-slate-400 text-sm mb-2">Authors</p>
							<div class="flex flex-col gap-2">
								{#each authorsList as author, idx}
									<div class="flex items-center gap-2">
										<input
											type="text"
											bind:value={authorsList[idx]}
											placeholder="Author name"
											class="flex-1 min-w-0 bg-transparent placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none border-b border-slate-200 dark:border-slate-600 py-0.5"
										/>
										<button
											type="button"
											onclick={() => { authorsList = authorsList.filter((_, i) => i !== idx); }}
											class="shrink-0 p-1 rounded text-slate-400 dark:text-slate-500 hover:text-red-500 dark:hover:text-red-400 transition-colors"
											aria-label="Remove author"
										>
											<Icon name="x" class="w-3.5 h-3.5" />
										</button>
									</div>
								{/each}
								<button
									type="button"
									onclick={() => { authorsList = [...authorsList, '']; }}
									class="self-start text-xs text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 transition-colors flex items-center gap-1 mt-1"
								>
									<Icon name="plus" class="w-3.5 h-3.5" />
									Add author
								</button>
							</div>
						</div>
						<!-- Description -->
						<div class="flex items-start px-4 py-3 gap-3">
							<label for="meta-desc-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0 pt-0.5">Description</label>
							<textarea
								id="meta-desc-{fingerprint}"
								bind:value={metaDraft.description}
								rows="2"
								class="flex-1 min-w-0 bg-transparent text-right resize-none focus:outline-none"
							></textarea>
						</div>
						<!-- Language -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-lang-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Language</label>
							<input id="meta-lang-{fingerprint}" type="text" bind:value={metaDraft.language}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none" />
						</div>
						<!-- Publisher -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-pub-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Publisher</label>
							<input id="meta-pub-{fingerprint}" type="text" bind:value={metaDraft.publisher}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none" />
						</div>
						<!-- Identifier -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-id-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Identifier</label>
							<input id="meta-id-{fingerprint}" type="text" bind:value={metaDraft.identifier}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none" />
						</div>
						<!-- Date -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-date-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Date</label>
							<input id="meta-date-{fingerprint}" type="text" bind:value={metaDraft.date}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none" />
						</div>
						<!-- Subject -->
						<div class="flex items-center px-4 py-3 gap-3">
							<label for="meta-subj-{fingerprint}" class="text-slate-500 dark:text-slate-400 shrink-0">Subject</label>
							<input id="meta-subj-{fingerprint}" type="text" bind:value={metaDraft.subject}
								class="flex-1 min-w-0 bg-transparent text-right placeholder:text-slate-300 dark:placeholder:text-slate-600 focus:outline-none" />
						</div>
					</div>
					{#if metaError}
						<p class="mt-2 text-xs text-red-500 dark:text-red-400">{metaError}</p>
					{/if}
				{:else}
					<dl class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
						{#if docMeta?.document_type}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Type</dt>
								<dd class="font-medium">{docMeta.document_type}</dd>
							</div>
						{/if}
						{#if docMeta?.language}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Language</dt>
								<dd class="font-medium">{docMeta.language}</dd>
							</div>
						{/if}
						{#if docMeta?.publisher}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Publisher</dt>
								<dd class="font-medium text-right break-words max-w-[60%]">{docMeta.publisher}</dd>
							</div>
						{/if}
						{#if docMeta?.identifier}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Identifier</dt>
								<dd class="font-mono text-xs">{docMeta.identifier}</dd>
							</div>
						{/if}
						{#if docMeta?.date}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Date</dt>
								<dd class="font-medium">{docMeta.date}</dd>
							</div>
						{/if}
						{#if docMeta?.subject}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Subject</dt>
								<dd class="font-medium text-right break-words max-w-[60%]">{docMeta.subject}</dd>
							</div>
						{/if}
						{#if !docMeta?.document_type && !docMeta?.language && !docMeta?.publisher && !docMeta?.identifier && !docMeta?.date && !docMeta?.subject}
							<div class="px-4 py-3">
								<p class="text-slate-400 dark:text-slate-500">No info set yet.</p>
							</div>
						{/if}
					</dl>
				{/if}
			</div>

		<!-- Tags -->
		<div>
			<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">Tags</h2>

			{#if doc.tags.length > 0}
				<div class="flex flex-wrap gap-1.5 mb-3">
					{#each doc.tags as tag}
						<span class="inline-flex items-center gap-1 pl-2.5 pr-1 py-1 rounded-lg text-xs font-medium bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300">
							{tag}
							<button
								onclick={() => removeTag(tag)}
								disabled={saving}
								aria-label="Remove tag {tag}"
								class="flex items-center justify-center w-4 h-4 rounded-full text-slate-400 dark:text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-600 hover:text-slate-600 dark:hover:text-slate-300 transition-colors disabled:opacity-40"
							>
								<Icon name="x" class="w-3 h-3" />
							</button>
						</span>
					{/each}
				</div>
			{:else}
				<p class="text-sm text-slate-400 dark:text-slate-500 mb-3">No tags.</p>
			{/if}

			<div class="flex gap-2">
				<div class="relative flex-1">
					<input
						type="text"
						list="tag-suggestions-{fingerprint}"
						bind:value={newTag}
						onkeydown={(e) => e.key === 'Enter' && addTag()}
						disabled={saving}
						placeholder="Add a tag…"
						class="w-full px-3 py-2 rounded-lg border border-slate-200 dark:border-slate-600
							bg-slate-50 dark:bg-slate-700/50 text-sm
							focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-transparent
							placeholder:text-slate-400 dark:placeholder:text-slate-500 disabled:opacity-50"
					/>
					<datalist id="tag-suggestions-{fingerprint}">
						{#each $allTags.filter((t) => !doc?.tags.includes(t)) as t}
							<option value={t}></option>
						{/each}
					</datalist>
				</div>
				<button
					onclick={addTag}
					disabled={saving || !newTag.trim()}
					class="inline-flex items-center gap-1.5 px-3 py-2 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
				>
					{#if saving}
						<Icon name="loader" class="w-4 h-4 animate-spin" />
					{:else}
						<Icon name="plus" class="w-4 h-4" />
					{/if}
					Add
				</button>
			</div>

			{#if tagError}
				<p class="mt-2 text-xs text-red-500 dark:text-red-400">{tagError}</p>
			{/if}
		</div>
	{/if}
</div>
