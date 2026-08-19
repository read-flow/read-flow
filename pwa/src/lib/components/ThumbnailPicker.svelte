<script lang="ts">
	// @feature: documents.change_thumbnail
	import Icon from '$lib/components/Icon.svelte';
	import {
		fetchPdfPageCount,
		fetchPdfPagePreviewUrl,
		savePdfPageThumbnail,
		type TrimMargins,
	} from '$lib/api/aggregator';
	import { refreshDocuments } from '$lib/stores/documents';

	interface Props {
		sourceId: number;
		guid: string;
		onclose: () => void;
	}

	let { sourceId, guid, onclose }: Props = $props();

	const FILMSTRIP_RADIUS = 4;
	const MAX_PADDING = 32;
	const DEFAULT_PADDING = 8;
	const MAX_MARGIN = 100;
	const NO_MARGINS: TrimMargins = { top: 0, bottom: 0, left: 0, right: 0 };

	let pageIndex = $state(0);
	let pageCount = $state<number | null>(null);
	let trim = $state(false);
	let padding = $state(DEFAULT_PADDING);
	let marginTop = $state(0);
	let marginBottom = $state(0);
	let marginLeft = $state(0);
	let marginRight = $state(0);
	let previewUrl = $state<string | null>(null);
	let filmstripUrls = $state<Record<number, string>>({});
	let saving = $state(false);
	let error = $state<string | null>(null);

	let previewGeneration = 0;

	async function loadBigPreview(
		idx: number,
		useTrim: boolean,
		usePadding: number,
		useMargins: TrimMargins,
	): Promise<void> {
		const myGeneration = ++previewGeneration;
		try {
			const url = await fetchPdfPagePreviewUrl(
				sourceId,
				guid,
				idx,
				useTrim,
				usePadding,
				useMargins,
				false,
			);
			if (myGeneration !== previewGeneration) {
				URL.revokeObjectURL(url);
				return;
			}
			const old = previewUrl;
			previewUrl = url;
			if (old) URL.revokeObjectURL(old);
		} catch (err) {
			if (myGeneration === previewGeneration) {
				error = err instanceof Error ? err.message : 'Failed to load preview.';
			}
		}
	}

	function loadFilmstripWindow(idx: number): void {
		if (pageCount === null) return;
		const start = Math.max(0, idx - FILMSTRIP_RADIUS);
		const end = Math.min(pageCount - 1, idx + FILMSTRIP_RADIUS);
		for (let i = start; i <= end; i++) {
			if (filmstripUrls[i]) continue;
			fetchPdfPagePreviewUrl(sourceId, guid, i, false, 0, NO_MARGINS, true)
				.then((url) => {
					filmstripUrls = { ...filmstripUrls, [i]: url };
				})
				.catch(() => {
					// Filmstrip thumbnails are best-effort; a missing tile just stays blank.
				});
		}
	}

	// Kick off the page count load once, on mount.
	$effect(() => {
		fetchPdfPageCount(sourceId, guid)
			.then((count) => {
				pageCount = count;
			})
			.catch((err) => {
				error = err instanceof Error ? err.message : 'Failed to load page count.';
			});
	});

	$effect(() => {
		if (pageCount === null) return;
		const idx = pageIndex;
		const useTrim = trim;
		const usePadding = padding;
		const useMargins: TrimMargins = {
			top: marginTop,
			bottom: marginBottom,
			left: marginLeft,
			right: marginRight,
		};
		loadBigPreview(idx, useTrim, usePadding, useMargins);
		loadFilmstripWindow(idx);
	});

	$effect(() => {
		return () => {
			if (previewUrl) URL.revokeObjectURL(previewUrl);
			Object.values(filmstripUrls).forEach((u) => URL.revokeObjectURL(u));
		};
	});

	const filmstripPages = $derived(
		pageCount === null
			? []
			: Array.from(
					{
						length:
							Math.min(pageCount - 1, pageIndex + FILMSTRIP_RADIUS) -
							Math.max(0, pageIndex - FILMSTRIP_RADIUS) +
							1,
					},
					(_, i) => Math.max(0, pageIndex - FILMSTRIP_RADIUS) + i,
				),
	);

	async function save(): Promise<void> {
		if (saving || !previewUrl) return;
		saving = true;
		error = null;
		try {
			await savePdfPageThumbnail(sourceId, guid, pageIndex, trim, padding, {
				top: marginTop,
				bottom: marginBottom,
				left: marginLeft,
				right: marginRight,
			});
			await refreshDocuments();
			onclose();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save thumbnail.';
		} finally {
			saving = false;
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onclick={onclose}>
	<div
		class="bg-white dark:bg-slate-800 rounded-2xl shadow-xl p-6 w-96 max-w-[90vw]"
		onclick={(e) => e.stopPropagation()}
	>
		<h2 class="text-base font-semibold mb-1">Change Thumbnail</h2>
		<p class="text-sm text-slate-500 dark:text-slate-400 mb-4">
			Choose a page to use as the thumbnail.
		</p>

		<div class="flex justify-center mb-3">
			<div
				class="w-40 h-56 rounded-lg bg-slate-100 dark:bg-slate-700 flex items-center justify-center overflow-hidden"
			>
				{#if previewUrl}
					<img src={previewUrl} alt="" class="max-w-full max-h-full object-contain" />
				{:else}
					<Icon name="loader" class="w-5 h-5 text-slate-400 dark:text-slate-500 animate-spin" />
				{/if}
			</div>
		</div>

		<div class="flex justify-center gap-1.5 mb-2 overflow-x-auto">
			{#each filmstripPages as idx}
				<button
					onclick={() => (pageIndex = idx)}
					aria-label="Page {idx + 1}"
					class="shrink-0 w-9 h-12 rounded overflow-hidden bg-slate-100 dark:bg-slate-700 flex items-center justify-center transition-shadow
						{idx === pageIndex ? 'ring-2 ring-accent' : 'hover:ring-1 hover:ring-slate-300 dark:hover:ring-slate-500'}"
				>
					{#if filmstripUrls[idx]}
						<img src={filmstripUrls[idx]} alt="" class="max-w-full max-h-full object-contain" />
					{/if}
				</button>
			{/each}
		</div>

		<p class="text-xs text-center text-slate-400 dark:text-slate-500 mb-4">
			{pageCount === null ? 'Loading…' : `Page ${pageIndex + 1} of ${pageCount}`}
		</p>

		<label class="flex items-center gap-2 mb-2 cursor-pointer select-none">
			<input
				type="checkbox"
				bind:checked={trim}
				class="accent-slate-900 dark:accent-slate-100"
			/>
			<span class="text-sm text-slate-600 dark:text-slate-300">Trim whitespace</span>
		</label>

		{#if trim}
			<div class="mb-4">
				<label
					for="thumbnail-padding-{sourceId}-{guid}"
					class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1"
				>
					<span>Padding</span>
					<span>{padding}px</span>
				</label>
				<input
					id="thumbnail-padding-{sourceId}-{guid}"
					type="range"
					min="0"
					max={MAX_PADDING}
					step="1"
					bind:value={padding}
					class="w-full accent-slate-900 dark:accent-slate-100"
				/>
			</div>

			<div class="mb-4">
				<p class="text-xs text-slate-500 dark:text-slate-400 mb-2">
					Exclude before cropping (e.g. a page number)
				</p>
				<div class="grid grid-cols-2 gap-x-4 gap-y-2">
					<div>
						<label
							for="thumbnail-margin-top-{sourceId}-{guid}"
							class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1"
						>
							<span>Top</span>
							<span>{marginTop}px</span>
						</label>
						<input
							id="thumbnail-margin-top-{sourceId}-{guid}"
							type="range"
							min="0"
							max={MAX_MARGIN}
							step="1"
							bind:value={marginTop}
							class="w-full accent-slate-900 dark:accent-slate-100"
						/>
					</div>
					<div>
						<label
							for="thumbnail-margin-bottom-{sourceId}-{guid}"
							class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1"
						>
							<span>Bottom</span>
							<span>{marginBottom}px</span>
						</label>
						<input
							id="thumbnail-margin-bottom-{sourceId}-{guid}"
							type="range"
							min="0"
							max={MAX_MARGIN}
							step="1"
							bind:value={marginBottom}
							class="w-full accent-slate-900 dark:accent-slate-100"
						/>
					</div>
					<div>
						<label
							for="thumbnail-margin-left-{sourceId}-{guid}"
							class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1"
						>
							<span>Left</span>
							<span>{marginLeft}px</span>
						</label>
						<input
							id="thumbnail-margin-left-{sourceId}-{guid}"
							type="range"
							min="0"
							max={MAX_MARGIN}
							step="1"
							bind:value={marginLeft}
							class="w-full accent-slate-900 dark:accent-slate-100"
						/>
					</div>
					<div>
						<label
							for="thumbnail-margin-right-{sourceId}-{guid}"
							class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1"
						>
							<span>Right</span>
							<span>{marginRight}px</span>
						</label>
						<input
							id="thumbnail-margin-right-{sourceId}-{guid}"
							type="range"
							min="0"
							max={MAX_MARGIN}
							step="1"
							bind:value={marginRight}
							class="w-full accent-slate-900 dark:accent-slate-100"
						/>
					</div>
				</div>
			</div>
		{/if}

		{#if error}
			<p class="text-sm text-red-500 dark:text-red-400 mb-3">{error}</p>
		{/if}

		<div class="flex gap-2 justify-end">
			<button
				onclick={onclose}
				disabled={saving}
				class="px-4 py-2 text-sm rounded-lg border border-slate-200 dark:border-slate-600
					text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-50"
			>
				Cancel
			</button>
			<button
				onclick={save}
				disabled={saving || !previewUrl}
				class="px-4 py-2 text-sm rounded-lg font-medium transition-colors
					bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900
					hover:bg-slate-700 dark:hover:bg-white
					disabled:opacity-50 disabled:cursor-not-allowed"
			>
				{saving ? 'Saving…' : 'Save'}
			</button>
		</div>
	</div>
</div>
