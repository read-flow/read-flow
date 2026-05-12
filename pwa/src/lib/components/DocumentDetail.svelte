<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { allDocuments, refreshDocuments, allTags } from '$lib/stores/documents';
	import { addTagsToFile, removeTagsFromFile } from '$lib/api/aggregator';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';

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

	async function removeTag(tag: string) {
		if (!doc || saving) return;
		doc = { ...doc, tags: doc.tags.filter((t) => t !== tag) };
		saving = true;
		tagError = '';
		try {
			await removeTagsFromFile(fingerprint, [tag]);
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
			await addTagsToFile(fingerprint, [tag]);
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
					<h2 class="text-base font-semibold text-slate-900 dark:text-slate-100 break-words leading-snug">
						{basename(doc.path)}
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
		{#if !onclose}
			<!-- Standalone page heading (no close button) -->
			<div>
				<h1 class="text-lg font-semibold text-slate-900 dark:text-slate-100 break-words">
					{basename(doc.path)}
				</h1>
				<p class="text-sm text-slate-400 dark:text-slate-500 mt-1 break-all">{doc.path}</p>
			</div>
		{/if}

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
							bg-slate-50 dark:bg-slate-700/50 text-slate-900 dark:text-slate-100 text-sm
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
