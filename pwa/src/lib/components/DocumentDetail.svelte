<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import {
		allDocuments,
		documentMetaMap,
		refreshDocuments,
		allTags,
	} from '$lib/stores/documents';
	import { addTagsToFile, removeTagsFromFile, updateDocumentMetadata } from '$lib/api/aggregator';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';
	import type { DocumentMeta, DocumentType } from '$lib/api/client';

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

	// ── User metadata editing ─────────────────────────────────────────────────
	let editingMeta = $state(false);
	let metaDraft = $state<DocumentMeta>({
		document_type: null, title: null, subtitle: null, authors: null, description: null,
		language: null, publisher: null, identifier: null, date: null, subject: null,
	});
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
				doc = docs.find((d) => d.fingerprint === fp) ?? null;
				loading = false;
			}
		})();
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
			doc = docs.find((d) => d.fingerprint === fingerprint) ?? doc;
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
			doc = docs.find((d) => d.fingerprint === fingerprint) ?? doc;
		} catch {
			tagError = `Failed to add tag "${tag}".`;
			const docs = get(allDocuments);
			doc = docs.find((d) => d.fingerprint === fingerprint) ?? doc;
		} finally {
			saving = false;
		}
	}
</script>

<div class="px-4 py-5 md:px-5 space-y-5">
	{#if onclose}
		<!-- Sidebar close button -->
		<div class="flex items-start justify-between gap-2">
			<div class="min-w-0">
				{#if loading}
					<div class="h-5 w-40 bg-slate-200 dark:bg-slate-700 rounded animate-pulse"></div>
				{:else if doc}
					{@const m = doc.document_guid ? $documentMetaMap.get(doc.document_guid) : undefined}
					<h2 class="text-base font-semibold break-words leading-snug">
						{m?.title ?? basename(doc.path)}
					</h2>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5 break-all">{doc.path}</p>
				{/if}
			</div>
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

		{#if !onclose}
			<!-- Standalone page heading (no close button) -->
			<div>
				<h1 class="text-lg font-semibold break-words">
					{docMeta?.title ?? basename(doc.path)}
				</h1>
				<p class="text-sm text-slate-400 dark:text-slate-500 mt-1 break-all">{doc.path}</p>
			</div>
		{/if}

		{@const allFormats = [doc, ...doc.otherFormats].filter((f) => f.type_ === 'epub' || f.type_ === 'pdf')}
		{#if allFormats.length > 0}
			<div class="flex flex-wrap gap-2">
				{#each allFormats as fmt}
					<a
						href={readerHref(fmt)}
						class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors"
					>
						<Icon name="library" class="w-4 h-4" />
						Open {fmt.type_.toUpperCase()}
					</a>
				{/each}
			</div>
		{/if}

		<!-- Reading status -->
		<div class="flex items-center justify-between">
			<span class="text-sm text-slate-500 dark:text-slate-400">Reading status</span>
			<span class="text-sm font-medium
				{doc.status === 'Read' ? 'text-green-600 dark:text-green-400'
				: doc.status === 'Reading' ? 'text-blue-600 dark:text-blue-400'
				: 'text-slate-500 dark:text-slate-400'}">
				{doc.status}
			</span>
		</div>

		<!-- Files -->
		<div>
			<h2 class="text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">Files</h2>
			<ul class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
				{#each [doc, ...doc.otherFormats] as fmt}
					<li class="flex items-center gap-3 px-4 py-3">
						<span class="shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium uppercase
							{fmt.type_ === 'pdf' ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
							: fmt.type_ === 'epub' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
							: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
							{fmt.type_}
						</span>
						<span class="flex-1 min-w-0 text-slate-700 dark:text-slate-300 truncate" title={fmt.path}>
							{basename(fmt.path)}
						</span>
						<span class="shrink-0 text-xs text-slate-400 dark:text-slate-500 tabular-nums">
							{formatSize(fmt.size)}
						</span>
					</li>
				{/each}
			</ul>
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
						{#if docMeta?.title}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Title</dt>
								<dd class="font-medium text-right break-words max-w-[60%]">{docMeta.title}</dd>
							</div>
						{/if}
						{#if docMeta?.subtitle}
							<div class="flex items-center justify-between px-4 py-3">
								<dt class="text-slate-500 dark:text-slate-400">Subtitle</dt>
								<dd class="font-medium text-right break-words max-w-[60%]">{docMeta.subtitle}</dd>
							</div>
						{/if}
						{#if docMeta?.authors?.length}
							<div class="flex items-start justify-between px-4 py-3 gap-4">
								<dt class="text-slate-500 dark:text-slate-400 shrink-0">Authors</dt>
								<dd class="flex flex-col items-end gap-0.5">
									{#each docMeta.authors as author}
										<span class="font-medium text-right">{author}</span>
									{/each}
								</dd>
							</div>
						{/if}
						{#if docMeta?.description}
							<div class="flex items-start justify-between px-4 py-3 gap-4">
								<dt class="text-slate-500 dark:text-slate-400 shrink-0">Description</dt>
								<dd class="text-right text-xs break-words">{docMeta.description}</dd>
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
						{#if !docMeta?.document_type && !docMeta?.title && !docMeta?.authors?.length}
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
							focus:outline-none focus:ring-2 focus:ring-slate-300 dark:focus:ring-slate-600 focus:border-transparent
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
