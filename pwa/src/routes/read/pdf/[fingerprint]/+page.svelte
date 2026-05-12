<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import * as pdfjsLib from 'pdfjs-dist';
	import type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist';
	import Icon from '$lib/components/Icon.svelte';
	import { allDocuments, refreshDocuments } from '$lib/stores/documents';
	import {
		fetchReadingProgress,
		saveReadingProgress,
		downloadFileFromSources,
	} from '$lib/api/aggregator';
	import { loadSources } from '$lib/stores/sources';
	import { db } from '$lib/db';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';

	// Set up the pdf.js worker once at module level.
	// new URL(..., import.meta.url) is resolved by Vite at build time.
	pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
		'pdfjs-dist/build/pdf.worker.min.mjs',
		import.meta.url,
	).href;

	const PREF_ZOOM_KEY = 'pdf-zoom';
	const PREF_MAXIMIZED_KEY = 'pdf-maximized';
	const PROGRESS_DEBOUNCE_MS = 2000;
	const RESIZE_DEBOUNCE_MS = 100;
	const TOOLBAR_HIDE_DELAY_MS = 3000;
	const SWIPE_THRESHOLD_PX = 50;

	// ── Route param ────────────────────────────────────────────────────────────
	const fingerprint = $derived($page.params.fingerprint ?? '');

	// ── Document & PDF state ───────────────────────────────────────────────────
	let doc = $state<AggregatedFile | null>(null);
	let pdfDoc = $state<PDFDocumentProxy | null>(null);
	let currentPage = $state(1);
	let totalPages = $state(0);
	let userScale = $state(1.0); // multiplier on top of fit-width
	let isLoading = $state(true);
	let loadError = $state<string | null>(null);
	let isRendering = false;

	// ── Toolbar visibility (mobile auto-hide) ──────────────────────────────────
	let toolbarVisible = $state(true);
	let toolbarTimer: ReturnType<typeof setTimeout> | null = null;
	let maximized = $state(false);

	// ── DOM refs ───────────────────────────────────────────────────────────────
	let canvasEl: HTMLCanvasElement | undefined = $state();
	let containerEl: HTMLDivElement | undefined = $state();

	// ── Timers ────────────────────────────────────────────────────────────────
	let progressTimer: ReturnType<typeof setTimeout> | null = null;
	let resizeTimer: ReturnType<typeof setTimeout> | null = null;
	let resizeObserver: ResizeObserver | null = null;

	// ── Touch tracking ────────────────────────────────────────────────────────
	let touchStartX = 0;

	// ── Helpers ───────────────────────────────────────────────────────────────
	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function clamp(n: number, min: number, max: number): number {
		return Math.max(min, Math.min(max, n));
	}

	// ── Rendering ─────────────────────────────────────────────────────────────
	async function renderPage(n: number): Promise<void> {
		if (!pdfDoc || !canvasEl || !containerEl || isRendering) return;
		isRendering = true;

		try {
			const pdfPage: PDFPageProxy = await pdfDoc.getPage(n);
			const unscaled = pdfPage.getViewport({ scale: 1 });
			const fitWidth = containerEl.clientWidth / unscaled.width;
			const viewport = pdfPage.getViewport({ scale: fitWidth * userScale });

			canvasEl.width = viewport.width;
			canvasEl.height = viewport.height;

			await pdfPage.render({ canvas: canvasEl, viewport }).promise;
		} finally {
			isRendering = false;
		}
	}

	// ── Navigation ────────────────────────────────────────────────────────────
	async function goToPage(n: number): Promise<void> {
		if (!pdfDoc) return;
		const target = clamp(n, 1, totalPages);
		if (target === currentPage) return;
		currentPage = target;
		await renderPage(target);
		scheduleProgressSave();
	}

	// ── Zoom ──────────────────────────────────────────────────────────────────
	async function adjustZoom(delta: number): Promise<void> {
		userScale = clamp(userScale + delta, 0.5, 3.0);
		await db.preferences.put({ key: PREF_ZOOM_KEY, value: String(userScale) });
		await renderPage(currentPage);
	}

	// ── Progress ──────────────────────────────────────────────────────────────
	function scheduleProgressSave(): void {
		if (progressTimer) clearTimeout(progressTimer);
		progressTimer = setTimeout(() => void saveProgress(), PROGRESS_DEBOUNCE_MS);
	}

	async function saveProgress(): Promise<void> {
		if (!doc || currentPage < 1) return;
		await saveReadingProgress({
			fingerprint: doc.fingerprint,
			progress: JSON.stringify({ page: currentPage }),
			last_updated: new Date().toISOString(),
		});
	}

	// ── Toolbar auto-hide (mobile) ────────────────────────────────────────────
	function resetToolbarTimer(): void {
		toolbarVisible = true;
		if (toolbarTimer) clearTimeout(toolbarTimer);
		toolbarTimer = setTimeout(() => {
			toolbarVisible = false;
		}, TOOLBAR_HIDE_DELAY_MS);
	}

	function handleCanvasTap(): void {
		resetToolbarTimer();
	}

	function toggleMaximized(): void {
		maximized = !maximized;
		void db.preferences.put({ key: PREF_MAXIMIZED_KEY, value: String(maximized) });
	}

	// ── Keyboard ──────────────────────────────────────────────────────────────
	function handleKeydown(e: KeyboardEvent): void {
		if (e.target instanceof HTMLInputElement) return;
		switch (e.key) {
			case 'ArrowRight':
			case 'ArrowDown':
			case ' ':
				e.preventDefault();
				void goToPage(currentPage + 1);
				break;
			case 'ArrowLeft':
			case 'ArrowUp':
				e.preventDefault();
				void goToPage(currentPage - 1);
				break;
			case 'Home':
				e.preventDefault();
				void goToPage(1);
				break;
			case 'End':
				e.preventDefault();
				void goToPage(totalPages);
				break;
			case 'm':
				e.preventDefault();
				toggleMaximized();
				break;
		}
	}

	// ── Touch / swipe ─────────────────────────────────────────────────────────
	function handleTouchStart(e: TouchEvent): void {
		touchStartX = e.touches[0].clientX;
	}

	function handleTouchEnd(e: TouchEvent): void {
		const dx = e.changedTouches[0].clientX - touchStartX;
		if (Math.abs(dx) >= SWIPE_THRESHOLD_PX) {
			void goToPage(dx < 0 ? currentPage + 1 : currentPage - 1);
		}
		resetToolbarTimer();
	}

	// ── Lifecycle ─────────────────────────────────────────────────────────────
	onMount(async () => {
		// Restore saved zoom preference
		const savedZoom = await db.preferences.get(PREF_ZOOM_KEY);
		if (savedZoom) {
			const parsed = parseFloat(savedZoom.value);
			if (isFinite(parsed)) userScale = clamp(parsed, 0.5, 3.0);
		}

		const savedMaximized = await db.preferences.get(PREF_MAXIMIZED_KEY);
		if (savedMaximized?.value === 'true') maximized = true;

		await loadSources();

		// Look up the document
		let docs = get(allDocuments);
		if (docs.length === 0) await refreshDocuments();
		docs = get(allDocuments);
		doc = docs.find((d) => d.fingerprint === fingerprint) ?? null;

		if (!doc) {
			loadError = 'Document not found. Make sure at least one source is configured and the library has loaded.';
			isLoading = false;
			return;
		}

		// Fetch saved reading progress
		let startPage = 1;
		try {
			const saved = await fetchReadingProgress(fingerprint);
			if (saved?.progress) {
				const parsed: unknown = JSON.parse(saved.progress);
				if (parsed && typeof parsed === 'object' && 'page' in parsed) {
					const p = (parsed as { page: unknown }).page;
					if (typeof p === 'number' && Number.isInteger(p) && p >= 1) startPage = p;
				}
			}
		} catch {
			// No saved progress or invalid format — start from page 1
		}

		// Download and load the PDF
		const fileName = basename(doc.path);
		try {
			const blob = await downloadFileFromSources(doc.sourceGuids, fileName);
			const data = await blob.arrayBuffer();
			pdfDoc = await pdfjsLib.getDocument({ data }).promise;
			totalPages = pdfDoc.numPages;
			currentPage = clamp(startPage, 1, totalPages);
			isLoading = false;

			// Attach resize observer — re-render on container width changes
			if (containerEl) {
				resizeObserver = new ResizeObserver(() => {
					if (resizeTimer) clearTimeout(resizeTimer);
					resizeTimer = setTimeout(() => void renderPage(currentPage), RESIZE_DEBOUNCE_MS);
				});
				resizeObserver.observe(containerEl);
			}

			await renderPage(currentPage);
		} catch (err) {
			loadError = err instanceof Error ? err.message : 'Failed to load PDF.';
			isLoading = false;
		}

		// Start the toolbar auto-hide timer on mobile
		resetToolbarTimer();
	});

	onDestroy(() => {
		// Flush any pending progress save before leaving
		if (progressTimer) {
			clearTimeout(progressTimer);
			void saveProgress();
		}
		if (toolbarTimer) clearTimeout(toolbarTimer);
		if (resizeTimer) clearTimeout(resizeTimer);
		resizeObserver?.disconnect();
		pdfDoc?.destroy();
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<svelte:head>
	<title>{doc ? basename(doc.path) : 'PDF'} — Read Flow</title>
</svelte:head>

<!--
	Fills the reader layout's h-dvh container (bg-slate-900).
	Toolbars slide off-screen on mobile after inactivity; on md+ they are always visible.
-->
<div
	class="relative flex flex-col h-full overflow-hidden"
	ontouchstart={handleTouchStart}
	ontouchend={handleTouchEnd}
	role="application"
	aria-label="PDF reader"
>
	<!-- ── Toolbar ────────────────────────────────────────────── -->
	<header
		class="absolute inset-x-0 top-0 flex items-center gap-3 px-4 py-3 bg-slate-800/95 backdrop-blur-sm z-10
			transition-transform duration-200
			{toolbarVisible ? 'translate-y-0' : '-translate-y-full'} {maximized ? 'md:-translate-y-full' : 'md:translate-y-0'}"
	>
		<a
			href="/documents/{fingerprint}"
			class="p-2 -ml-2 rounded-lg text-white hover:bg-slate-700 transition-colors shrink-0"
			aria-label="Back to document details"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</a>

		<span class="flex-1 text-sm font-medium text-slate-200 truncate min-w-0">
			{doc ? basename(doc.path) : 'Loading…'}
		</span>

		<!-- Zoom controls (always shown; smaller on mobile) -->
		<div class="flex items-center gap-0.5 shrink-0">
			<button
				onclick={() => void adjustZoom(-0.25)}
				disabled={userScale <= 0.5}
				class="p-1.5 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-30"
				aria-label="Zoom out"
			>
				<Icon name="minus" class="w-4 h-4" />
			</button>
			<span class="text-xs text-slate-400 w-11 text-center tabular-nums">
				{Math.round(userScale * 100)}%
			</span>
			<button
				onclick={() => void adjustZoom(0.25)}
				disabled={userScale >= 3.0}
				class="p-1.5 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-30"
				aria-label="Zoom in"
			>
				<Icon name="plus" class="w-4 h-4" />
			</button>
		</div>

		<button
			onclick={toggleMaximized}
			class="hidden md:flex p-1.5 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors shrink-0"
			aria-label={maximized ? 'Restore toolbar' : 'Maximize viewer'}
		>
			<Icon name={maximized ? 'minimize' : 'maximize'} class="w-4 h-4" />
		</button>
	</header>

	<!-- ── Canvas area ───────────────────────────────────────── -->
	<div
		bind:this={containerEl}
		class="flex-1 overflow-y-auto bg-slate-700 flex flex-col items-center py-4"
		onclick={handleCanvasTap}
		role="presentation"
	>
		{#if isLoading}
			<div class="flex items-center gap-2.5 m-auto text-slate-300">
				<Icon name="loader" class="w-6 h-6 animate-spin" />
				<span class="text-sm">Loading PDF…</span>
			</div>
		{:else if loadError}
			<div class="flex flex-col items-center gap-3 m-auto text-center px-8 max-w-sm">
				<Icon name="alert-circle" class="w-10 h-10 text-red-400 shrink-0" />
				<p class="text-sm text-slate-300 leading-relaxed">{loadError}</p>
				<a
					href="/documents/{fingerprint}"
					class="text-sm text-slate-400 hover:text-slate-200 underline transition-colors"
				>
					Back to document
				</a>
			</div>
		{:else}
			<canvas
				bind:this={canvasEl}
				class="shadow-2xl bg-white max-w-full"
				aria-label="PDF page {currentPage} of {totalPages}"
			></canvas>
		{/if}
	</div>

	<!-- ── Bottom navigation ─────────────────────────────────── -->
	<footer
		class="absolute inset-x-0 bottom-0 flex items-center justify-between px-6 bg-slate-800/95 backdrop-blur-sm z-10
			transition-transform duration-200
			{toolbarVisible ? 'translate-y-0' : 'translate-y-full'} {maximized ? 'md:translate-y-full' : 'md:translate-y-0'}"
		style="padding-top: 0.75rem; padding-bottom: max(0.75rem, env(safe-area-inset-bottom, 0px))"
	>
		<button
			onclick={() => void goToPage(currentPage - 1)}
			disabled={currentPage <= 1 || !pdfDoc}
			class="p-2 rounded-lg text-white hover:bg-slate-700 transition-colors disabled:opacity-30 min-w-[44px] min-h-[44px] flex items-center justify-center"
			aria-label="Previous page"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</button>

		<span class="text-sm text-slate-400 tabular-nums select-none">
			{#if totalPages > 0}
				{currentPage} / {totalPages}
			{:else}
				–
			{/if}
		</span>

		<button
			onclick={() => void goToPage(currentPage + 1)}
			disabled={currentPage >= totalPages || !pdfDoc}
			class="p-2 rounded-lg text-white hover:bg-slate-700 transition-colors disabled:opacity-30 min-w-[44px] min-h-[44px] flex items-center justify-center"
			aria-label="Next page"
		>
			<Icon name="chevron-down" class="w-5 h-5 -rotate-90" />
		</button>
	</footer>

	{#if maximized}
		<button
			onclick={toggleMaximized}
			class="hidden md:flex absolute top-3 right-3 z-20 p-1.5 rounded bg-slate-800/70 text-slate-400
				hover:text-white hover:bg-slate-700/90 transition-all"
			aria-label="Restore toolbar"
		>
			<Icon name="minimize" class="w-4 h-4" />
		</button>
	{/if}
</div>
